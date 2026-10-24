use std::path::{Path, PathBuf};

use crate::{
  Language, ProcessBuilder, Result, Shell, hint,
  manifest::{BuildType, Project, TestRunner, TestSuite},
  shell::{elapsed, style},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestOptions {
  pub junit: Option<PathBuf>,
  pub suites: Vec<String>,
  pub build_type: BuildType,
  pub build_dir: Option<PathBuf>,
  pub extra: Vec<String>,
  pub color: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteReport {
  pub name: String,
  pub runner: TestRunner,
  pub success: bool,
  pub code: Option<i32>,
  pub elapsed: std::time::Duration,
  pub junit: Option<String>,
}

#[derive(Debug)]
pub struct Tester {
  suites: Vec<PlannedSuite>,
  junit: Option<PathBuf>,
}

impl Tester {
  pub fn plan(project: &Project, options: &TestOptions) -> Result<Self> {
    let ci_seed = std::env::var("CI_PIPELINE_IID").ok();
    let ctx = SuiteContext {
      project,
      options,
      ci_seed: ci_seed.as_deref().filter(|s| !s.is_empty()),
      headless: std::env::var_os("CI").is_some(),
    };
    let suites = Self::select(project, &options.suites)?
      .iter()
      .map(|s| ctx.plan(s))
      .collect::<Result<_>>()?;
    Ok(Self {
      suites,
      junit: options.junit.as_ref().map(|dir| project.join(dir)),
    })
  }

  pub fn select(project: &Project, names: &[String]) -> Result<Vec<TestSuite>> {
    let declared = &project.manifest.test.suites;
    let suites = if declared.is_empty() {
      Self::inferred(project)
    } else {
      declared.clone()
    };
    let known: Vec<_> = suites.iter().map(TestSuite::display_name).collect();
    if let Some(name) = names.iter().find(|n| !known.contains(n)) {
      return Err(hint(
        format!("unknown test suite `{name}`"),
        format!("expected one of: {}", known.join(", ")),
      ));
    }
    Ok(
      suites
        .into_iter()
        .filter(|s| names.is_empty() || names.contains(&s.display_name()))
        .collect(),
    )
  }

  fn inferred(project: &Project) -> Vec<TestSuite> {
    let bare = |runner| TestSuite {
      name: None,
      runner,
      profile: None,
      binary: None,
      build_dir: None,
      filter: None,
      args: Vec::new(),
      env: std::collections::BTreeMap::new(),
    };
    let mut suites = Vec::new();
    if project.manifest.has(Language::Rust) {
      suites.push(bare(if which::which("cargo-nextest").is_ok() {
        TestRunner::Nextest
      } else {
        TestRunner::Cargo
      }));
    }
    if project.manifest.has_c_family() {
      suites.push(bare(TestRunner::Ctest));
    }
    suites
  }

  pub fn is_empty(&self) -> bool {
    self.suites.is_empty()
  }

  pub fn run(&self, shell: &mut Shell) -> Result<Vec<SuiteReport>> {
    let reports = self
      .suites
      .iter()
      .map(|suite| suite.run(shell))
      .collect::<Result<Vec<_>>>()?;
    if let Some(dir) = &self.junit {
      let mut merged = JunitReport::default();
      for xml in reports.iter().filter_map(|r| r.junit.as_deref()) {
        merged.add(xml);
      }
      let path = dir.join("junit.xml");
      fs_err::create_dir_all(dir)?;
      fs_err::write(&path, merged.render())?;
      shell.status("Writing", path.display())?;
    }
    let width = reports
      .iter()
      .map(|r| r.name.len())
      .max()
      .unwrap_or_default();
    for report in &reports {
      let (status, style) = if report.success {
        ("ok", style::HEADER)
      } else {
        ("FAILED", style::ERROR)
      };
      shell.status_with_style(
        status,
        format_args!(
          "{:<width$}  {:<7}  {}",
          report.name,
          report.runner.to_string(),
          elapsed(report.elapsed)
        ),
        &style,
      )?;
    }
    Ok(reports)
  }
}

#[derive(Debug, Clone)]
struct PlannedSuite {
  name: String,
  runner: TestRunner,
  process: ProcessBuilder,
  junit: Option<PathBuf>,
  generated: Vec<(PathBuf, String)>,
}

impl PlannedSuite {
  fn new(
    name: String,
    runner: TestRunner,
    process: ProcessBuilder,
    junit: Option<PathBuf>,
  ) -> Self {
    Self {
      name,
      runner,
      process,
      junit,
      generated: Vec::new(),
    }
  }

  fn run(&self, shell: &mut Shell) -> Result<SuiteReport> {
    shell.status("Testing", format_args!("{} ({})", self.name, self.runner))?;
    shell.verbose(|s| s.status("Running", &self.process))?;
    for path in self
      .generated
      .iter()
      .map(|(path, _)| path)
      .chain(&self.junit)
    {
      let _ = std::fs::remove_file(path);
      if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
      }
    }
    for (path, contents) in &self.generated {
      fs_err::write(path, contents)?;
    }
    let start = std::time::Instant::now();
    let status = self.process.status_unchecked()?;
    let elapsed = start.elapsed();
    let junit = self.junit.as_ref().map(|path| {
      std::fs::read_to_string(path).unwrap_or_else(|_| {
        JunitReport::failure(&self.name, &format!("no JUnit report ({status})"))
      })
    });
    Ok(SuiteReport {
      name: self.name.clone(),
      runner: self.runner,
      success: status.success(),
      code: status.code(),
      elapsed,
      junit,
    })
  }
}

#[derive(Debug, Clone, Copy)]
struct SuiteContext<'a> {
  project: &'a Project,
  options: &'a TestOptions,
  ci_seed: Option<&'a str>,
  headless: bool,
}

impl SuiteContext<'_> {
  const NEXTEST_CONFIG: &'static str = ".config/nextest.toml";
  const HEADLESS_ENV: (&'static str, &'static str) = ("QT_QPA_PLATFORM", "offscreen");

  fn root(&self) -> &Path {
    &self.project.root
  }

  fn plan(&self, suite: &TestSuite) -> Result<PlannedSuite> {
    let name = suite.display_name();
    let mut planned = match suite.runner {
      TestRunner::Cargo if self.options.junit.is_some() => {
        if which::which("cargo-nextest").is_err() {
          return Err(hint(
            format!("test suite `{name}` runs `cargo test`, which cannot write JUnit reports"),
            "install cargo-nextest (`cargo install cargo-nextest --locked`) or use `runner = \"nextest\"`",
          ));
        }
        self.nextest(suite, name)
      }
      TestRunner::Cargo => self.cargo(suite, name),
      TestRunner::Nextest => self.nextest(suite, name),
      TestRunner::Ctest => self.ctest(suite, name)?,
      TestRunner::Catch2 => self.catch2(suite, name)?,
    };
    for (key, value) in &suite.env {
      planned.process = planned.process.env(key, value);
    }
    let (key, value) = Self::HEADLESS_ENV;
    if self.headless && !suite.env.contains_key(key) && std::env::var_os(key).is_none() {
      planned.process = planned.process.env(key, value);
    }
    Ok(planned)
  }

  fn build_dir(&self, suite: &TestSuite) -> Result<PathBuf> {
    let Some(template) = &suite.build_dir else {
      return self.options.build_dir.clone().ok_or_else(|| {
        hint(
          "the project has no CMake build directory to test in",
          "set `build-dir` in `[[test.suite]]`",
        )
      });
    };
    let dir = self
      .project
      .join(template.replace("{build-type}", &self.options.build_type.to_string()));
    if dir.is_dir() {
      Ok(dir)
    } else {
      Err(hint(
        format!("build directory `{}` does not exist", dir.display()),
        "check `build-dir` in `[[test.suite]]`",
      ))
    }
  }

  fn junit_file(&self, name: &str) -> Option<PathBuf> {
    self.options.junit.as_ref().map(|dir| {
      self
        .root()
        .join(dir)
        .join("suites")
        .join(format!("{name}.xml"))
    })
  }

  fn cargo(&self, suite: &TestSuite, name: String) -> PlannedSuite {
    let mut process = self
      .project
      .manifest
      .build
      .cargo
      .command(self.root(), &["test", "--workspace"]);
    if self.options.build_type.is_optimized() {
      process = process.arg("--release");
    }
    process = process.args(&suite.args);
    if !self.options.extra.is_empty() {
      process = process.arg("--").args(&self.options.extra);
    }
    PlannedSuite::new(name, TestRunner::Cargo, process, None)
  }

  fn nextest(&self, suite: &TestSuite, name: String) -> PlannedSuite {
    let root = self.root();
    let mut process = self
      .project
      .manifest
      .build
      .cargo
      .command(root, &["nextest", "run", "--workspace"]);
    let mut generated = Vec::new();
    let mut junit = None;
    let profile = match (&suite.profile, self.junit_file(&name)) {
      (profile, None) => profile.as_deref(),
      (profile, Some(own)) => {
        let profile = profile.as_deref().unwrap_or("ci");
        let path = Self::nextest_junit_path(root, profile).unwrap_or_else(|| {
          generated.push((
            own.with_extension("nextest.toml"),
            format!("[profile.{profile}.junit]\npath = \"junit.xml\"\n"),
          ));
          "junit.xml".to_owned()
        });
        let target = std::env::var_os("CARGO_TARGET_DIR")
          .map_or_else(|| root.join("target"), |dir| root.join(dir));
        junit = Some(target.join("nextest").join(profile).join(path));
        Some(profile)
      }
    };
    for (config, _) in &generated {
      process = process
        .arg("--tool-config-file")
        .arg(format!("wkit:{}", config.display()));
    }
    if let Some(profile) = profile {
      process = process.args(["--profile", profile]);
    }
    if self.options.build_type.is_optimized() {
      process = process.arg("--release");
    }
    process = process.args(&suite.args).args(&self.options.extra);
    PlannedSuite {
      generated,
      ..PlannedSuite::new(name, TestRunner::Nextest, process, junit)
    }
  }

  fn nextest_junit_path(root: &Path, profile: &str) -> Option<String> {
    let config: toml::Table =
      toml::from_str(&std::fs::read_to_string(root.join(Self::NEXTEST_CONFIG)).ok()?).ok()?;
    config
      .get("profile")?
      .get(profile)?
      .get("junit")?
      .get("path")?
      .as_str()
      .map(ToOwned::to_owned)
  }

  fn ctest(&self, suite: &TestSuite, name: String) -> Result<PlannedSuite> {
    let mut process = ProcessBuilder::new("ctest")
      .cwd(self.root())
      .arg("--test-dir")
      .arg(self.build_dir(suite)?)
      .args(["--output-on-failure", "--build-config"])
      .arg(self.options.build_type.to_string());
    if let Some(filter) = &suite.filter {
      process = process.args(["-R", filter]);
    }
    let junit = self.junit_file(&name);
    if let Some(junit) = &junit {
      process = process.arg("--output-junit").arg(junit);
    }
    process = process.args(&suite.args).args(&self.options.extra);
    Ok(PlannedSuite::new(name, TestRunner::Ctest, process, junit))
  }

  fn catch2(&self, suite: &TestSuite, name: String) -> Result<PlannedSuite> {
    let dir = self.build_dir(suite)?;
    let binary = suite.binary.as_deref().unwrap_or(&name);
    let path = crate::fs::find_executable(&dir, binary).ok_or_else(|| {
      hint(
        format!("test binary `{binary}` not found in `{}`", dir.display()),
        "check `binary` in `[[test.suite]]`",
      )
    })?;
    let mut process = ProcessBuilder::new(path).cwd(self.root());
    if let Some(filter) = &suite.filter {
      process = process.arg(filter);
    }
    if let Some(seed) = self.ci_seed {
      process = process.args(["--order", "rand", "--rng-seed", seed]);
    }
    let colour = if self.options.color { "ansi" } else { "none" };
    let junit = self.junit_file(&name);
    process = match &junit {
      Some(junit) => process
        .arg("--reporter")
        .arg(format!("JUnit::out={}", junit.display()))
        .arg("--reporter")
        .arg(format!("console::out=-::colour-mode={colour}")),
      None => process.args(["--colour-mode", colour]),
    };
    process = process.args(&suite.args).args(&self.options.extra);
    Ok(PlannedSuite::new(name, TestRunner::Catch2, process, junit))
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct JunitReport {
  suites: Vec<String>,
}

impl JunitReport {
  fn add(&mut self, xml: &str) {
    let body = Self::strip_prolog(xml);
    let Some(rest) = body.strip_prefix("<testsuites") else {
      if !body.is_empty() {
        self.suites.push(body.to_owned());
      }
      return;
    };
    if let Some(open_end) = rest.find('>')
      && !rest[..open_end].ends_with('/')
    {
      let inner = &rest[open_end + 1..];
      let inner = inner
        .rfind("</testsuites>")
        .map_or(inner, |end| &inner[..end]);
      self.suites.push(inner.trim().to_owned());
    }
  }

  fn failure(name: &str, message: &str) -> String {
    let name = Self::escape(name);
    let message = Self::escape(message);
    format!(
      "<testsuite name=\"{name}\" tests=\"1\" failures=\"1\" errors=\"0\"><testcase name=\"{name}\" classname=\"{name}\"><failure message=\"{message}\"/></testcase></testsuite>"
    )
  }

  fn render(&self) -> String {
    let suites: String = self
      .suites
      .iter()
      .flat_map(|suite| [suite.as_str(), "\n"])
      .collect();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n{suites}</testsuites>\n")
  }

  fn strip_prolog(xml: &str) -> &str {
    let mut body = xml.trim_start_matches('\u{feff}').trim();
    loop {
      let skip = if body.starts_with("<?") {
        body.find("?>").map(|end| end + 2)
      } else if body.starts_with("<!--") {
        body.find("-->").map(|end| end + 3)
      } else {
        None
      };
      match skip {
        Some(end) => body = body[end..].trim_start(),
        None => return body.trim(),
      }
    }
  }

  fn escape(text: &str) -> String {
    text
      .replace('&', "&amp;")
      .replace('<', "&lt;")
      .replace('>', "&gt;")
      .replace('"', "&quot;")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  fn args(suite: &PlannedSuite) -> Vec<String> {
    suite
      .process
      .get_args()
      .iter()
      .map(|a| a.to_string_lossy().into_owned())
      .collect()
  }

  #[test]
  fn infers_ctest_for_c_family_projects() {
    let (dir, project) = project("[project]\nlanguages = [\"cpp\"]\n");
    let missing = Tester::plan(&project, &TestOptions::default()).unwrap_err();
    assert!(
      missing.to_string().contains("no CMake build directory"),
      "{missing}"
    );

    let build = dir.path().join("build/Debug");
    let options = TestOptions {
      junit: Some("build/junit".into()),
      build_dir: Some(build.clone()),
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    let [suite] = &tester.suites[..] else {
      panic!("{:?}", tester.suites);
    };
    assert_eq!(suite.runner, TestRunner::Ctest);
    let args = args(suite);
    assert_eq!(
      args[..2],
      ["--test-dir".to_owned(), build.display().to_string()]
    );
    assert!(args.contains(&"--output-on-failure".to_owned()));
    assert_eq!(
      suite.junit,
      Some(dir.path().join("build/junit/suites/ctest.xml"))
    );
  }

  #[test]
  fn catch2_suites_find_their_binary_and_write_junit() {
    let (dir, project) = project(
      "[project]\nlanguages = [\"cpp\"]\n\n[[test.suite]]\nrunner = \"catch2\"\nbinary = \"unit\"\nfilter = \"[fast]\"\nbuild-dir = \"build/{build-type}\"\n\n[[test.suite]]\nrunner = \"ctest\"\nname = \"all\"\nbuild-dir = \"build/{build-type}\"\n",
    );
    let bin = dir.path().join("build/Release/tests");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("unit{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&exe, "").unwrap();
    let options = TestOptions {
      junit: Some("out".into()),
      suites: vec!["unit".into()],
      build_type: BuildType::Release,
      extra: vec!["--durations".into(), "yes".into()],
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    let [suite] = &tester.suites[..] else {
      panic!("{:?}", tester.suites);
    };
    assert_eq!(suite.process.get_program(), exe.as_os_str());
    assert_eq!(
      args(suite),
      [
        "[fast]",
        "--reporter",
        &format!(
          "JUnit::out={}",
          dir.path().join("out/suites/unit.xml").display()
        ),
        "--reporter",
        "console::out=-::colour-mode=none",
        "--durations",
        "yes"
      ]
    );

    let unknown = TestOptions {
      suites: vec!["nope".into()],
      ..TestOptions::default()
    };
    let err = Tester::plan(&project, &unknown).unwrap_err();
    assert!(
      err.to_string().contains("expected one of: unit, all"),
      "{err}"
    );
  }

  #[test]
  fn nextest_reads_the_junit_path_from_its_profile() {
    let (dir, project) = project(
      "[project]\nlanguages = [\"rust\"]\n\n[[test.suite]]\nrunner = \"nextest\"\nenv = { RUST_LOG = \"debug\" }\n",
    );
    let options = TestOptions {
      junit: Some("build/junit".into()),
      build_type: BuildType::Release,
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    let suite = &tester.suites[0];
    let config = dir.path().join("build/junit/suites/nextest.nextest.toml");
    assert_eq!(
      suite.generated,
      [(
        config.clone(),
        "[profile.ci.junit]\npath = \"junit.xml\"\n".to_owned()
      )]
    );
    assert!(args(suite).ends_with(&[
      "--tool-config-file".to_owned(),
      format!("wkit:{}", config.display()),
      "--profile".into(),
      "ci".into(),
      "--release".into()
    ]));
    assert!(
      suite
        .junit
        .as_ref()
        .unwrap()
        .ends_with("nextest/ci/junit.xml")
    );

    std::fs::create_dir_all(dir.path().join(".config")).unwrap();
    std::fs::write(
      dir.path().join(SuiteContext::NEXTEST_CONFIG),
      "[profile.ci.junit]\npath = \"report.xml\"\n",
    )
    .unwrap();
    let tester = Tester::plan(&project, &options).unwrap();
    let suite = &tester.suites[0];
    assert!(suite.generated.is_empty());
    assert!(
      suite
        .junit
        .as_ref()
        .unwrap()
        .ends_with("nextest/ci/report.xml")
    );
    assert_eq!(
      suite.process.get_env("RUST_LOG"),
      Some(std::ffi::OsStr::new("debug"))
    );
  }

  #[test]
  fn plain_cargo_passes_extra_args_to_the_harness() {
    let (_dir, project) =
      project("[project]\nlanguages = [\"rust\"]\n\n[[test.suite]]\nrunner = \"cargo\"\n");
    let options = TestOptions {
      extra: vec!["--nocapture".into()],
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    assert!(args(&tester.suites[0]).ends_with(&[
      "test".to_owned(),
      "--workspace".into(),
      "--".into(),
      "--nocapture".into()
    ]));
  }

  #[test]
  fn failed_suites_still_land_in_the_merged_report() {
    let (dir, project) = project(
      "[project]\nlanguages = [\"c\"]\n\n[[test.suite]]\nrunner = \"ctest\"\nname = \"smoke\"\nbuild-dir = \".\"\n",
    );
    let options = TestOptions {
      junit: Some(dir.path().join("junit")),
      ..TestOptions::default()
    };
    let mut tester = Tester::plan(&project, &options).unwrap();
    tester.suites[0].process = ProcessBuilder::new("false");
    let reports = tester.run(&mut Shell::sink()).unwrap();
    assert!(!reports[0].success);
    let merged = std::fs::read_to_string(dir.path().join("junit/junit.xml")).unwrap();
    assert!(merged.contains("<testsuite name=\"smoke\""), "{merged}");
  }

  #[test]
  fn headless_ci_defaults_qt_to_offscreen() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"rust\"]\n\n[[test.suite]]\nrunner = \"cargo\"\n\n[[test.suite]]\nrunner = \"nextest\"\nenv = { QT_QPA_PLATFORM = \"xcb\" }\n",
    );
    let options = TestOptions::default();
    let ctx = SuiteContext {
      project: &project,
      options: &options,
      ci_seed: None,
      headless: true,
    };
    let [default, pinned] = &project.manifest.test.suites[..] else {
      panic!();
    };
    let platform = |suite| {
      ctx
        .plan(suite)
        .unwrap()
        .process
        .get_env("QT_QPA_PLATFORM")
        .map(|v| v.to_string_lossy().into_owned())
    };
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
      assert_eq!(platform(default).as_deref(), Some("offscreen"));
    }
    assert_eq!(platform(pinned).as_deref(), Some("xcb"));
  }

  #[test]
  fn junit_reports_merge_suites_from_every_runner() {
    let mut report = JunitReport::default();
    report.add("<?xml version=\"1.0\"?>\n<testsuites name=\"nextest\" tests=\"2\">\n  <testsuite name=\"a\"/>\n  <testsuite name=\"b\"/>\n</testsuites>\n");
    report.add("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- ctest -->\n<testsuite name=\"ctest\" tests=\"1\"><testcase name=\"t\"/></testsuite>");
    report.add("<testsuites/>");
    report.add(&JunitReport::failure("unit", "exited with \"code\" 2"));
    let xml = report.render();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n"));
    assert_eq!(xml.matches("<testsuites").count(), 1);
    assert_eq!(xml.matches("<testsuite ").count(), 4);
    assert!(xml.contains("message=\"exited with &quot;code&quot; 2\""));
    assert!(xml.ends_with("</testsuites>\n"));
  }
}
