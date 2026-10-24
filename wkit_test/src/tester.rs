use wkit_core::{Shell, shell::Elapsed};
use wkit_manifest::{BuildType, Project, TestSuite};

use crate::{Error, JunitReport, PlannedSuite, Result, SuiteContext, SuiteReport, TestOptions};

#[derive(Debug)]
pub struct Tester {
  suites: Vec<PlannedSuite>,
  junit: Option<std::path::PathBuf>,
}

impl Tester {
  pub const CI_SEED_ENV: &str = "CI_PIPELINE_IID";
  pub const JUNIT_FILE: &str = "junit.xml";

  pub fn plan(project: &Project, options: &TestOptions) -> Result<Self> {
    let ci_seed = std::env::var(Self::CI_SEED_ENV).ok();
    let ctx = SuiteContext {
      project,
      options,
      build_type: if options.release {
        BuildType::Release
      } else {
        project.manifest.build.build_type
      },
      ci_seed: ci_seed.as_deref().filter(|s| !s.is_empty()),
      headless: std::env::var_os("CI").is_some(),
    };
    let declared = &project.manifest.test.suites;
    let suites: Vec<TestSuite> = if declared.is_empty() {
      PlannedSuite::inferred(&ctx)
    } else {
      declared.clone()
    };
    let known: Vec<_> = suites.iter().map(TestSuite::display_name).collect();
    for name in &options.suites {
      if !known.contains(name) {
        return Err(Error::setup(
          format!("unknown test suite `{name}`"),
          Some(format!("expected one of: {}", known.join(", "))),
        ));
      }
    }
    let suites = suites
      .iter()
      .filter(|s| options.suites.is_empty() || options.suites.contains(&s.display_name()))
      .map(|s| PlannedSuite::plan(&ctx, s))
      .collect::<Result<_>>()?;
    let junit = options.junit.as_ref().map(|dir| project.join(dir));
    Ok(Self { suites, junit })
  }

  pub fn suites(&self) -> &[PlannedSuite] {
    &self.suites
  }

  pub fn run(&self, shell: &mut Shell) -> Result<Vec<SuiteReport>> {
    let mut reports = Vec::with_capacity(self.suites.len());
    for suite in &self.suites {
      reports.push(suite.run(shell)?);
    }
    if let Some(dir) = &self.junit {
      let mut merged = JunitReport::default();
      for xml in reports.iter().filter_map(|r| r.junit.as_deref()) {
        merged.add(xml);
      }
      let path = dir.join(Self::JUNIT_FILE);
      std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&path, merged.render()))
        .map_err(|source| wkit_core::Error::Write {
          path: path.clone(),
          source,
        })?;
      shell.status("Writing", path.display())?;
    }
    Self::summarize(shell, &reports)?;
    Ok(reports)
  }

  fn summarize(shell: &mut Shell, reports: &[SuiteReport]) -> Result<()> {
    let width = reports
      .iter()
      .map(|r| r.name.len())
      .max()
      .unwrap_or_default();
    for report in reports {
      let (status, style) = if report.success {
        ("ok", wkit_core::shell::style::HEADER)
      } else {
        ("FAILED", wkit_core::shell::style::ERROR)
      };
      shell.status_with_style(
        status,
        format_args!(
          "{:<width$}  {:<7}  {}",
          report.name,
          report.runner.to_string(),
          Elapsed(report.elapsed)
        ),
        &style,
      )?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use wkit_manifest::TestRunner;

  use super::*;
  use crate::BuildDir;

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
      missing.to_string().contains("help: configure and build"),
      "{missing}"
    );

    let build = dir.path().join("build/Debug");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join(BuildDir::CTEST_FILE), "").unwrap();
    let options = TestOptions {
      junit: Some("build/junit".into()),
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    let [suite] = tester.suites() else {
      panic!("{:?}", tester.suites());
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
      "[project]\nlanguages = [\"cpp\"]\n\n[build]\nbuild-type = \"release\"\n\n[[test.suite]]\nrunner = \"catch2\"\nbinary = \"unit\"\nfilter = \"[fast]\"\nbuild-dir = \"build/{build-type}\"\n\n[[test.suite]]\nrunner = \"ctest\"\nname = \"all\"\nbuild-dir = \"build/{build-type}\"\n",
    );
    let bin = dir.path().join("build/Release/tests");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("unit{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&exe, "").unwrap();
    let options = TestOptions {
      junit: Some("out".into()),
      suites: vec!["unit".into()],
      extra: vec!["--durations".into(), "yes".into()],
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    let [suite] = tester.suites() else {
      panic!("{:?}", tester.suites());
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
      release: true,
      ..TestOptions::default()
    };
    let tester = Tester::plan(&project, &options).unwrap();
    let suite = &tester.suites()[0];
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
      dir.path().join(PlannedSuite::NEXTEST_CONFIG),
      "[profile.ci.junit]\npath = \"report.xml\"\n",
    )
    .unwrap();
    let tester = Tester::plan(&project, &options).unwrap();
    let suite = &tester.suites()[0];
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
    assert!(args(&tester.suites()[0]).ends_with(&[
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
    tester.suites[0].process = wkit_core::ProcessBuilder::new("false");
    let mut shell = Shell::from_write(Box::new(std::io::sink()));
    let reports = tester.run(&mut shell).unwrap();
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
      build_type: BuildType::Debug,
      ci_seed: None,
      headless: true,
    };
    let [default, pinned] = &project.manifest.test.suites[..] else {
      panic!();
    };
    let platform = |suite| {
      PlannedSuite::plan(&ctx, suite)
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
}
