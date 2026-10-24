use std::path::PathBuf;

use anyhow::Context as _;
use rayon::prelude::*;

use super::Run;
use crate::{
  Failure, Language, ProcessBuilder, Result,
  build::{BuildOptions, Builder},
  check::Checker,
  cli::Context,
  detect::{BuildSystem, RustfmtChannel},
  format::{Formatter, Kind, Rustfmt, ToolFormatter},
  manifest::{Project, TestRunner},
  package::Distro,
  shell::{plural, style},
  tool::Tool,
  version::{Flavor, VersionSet},
};

#[derive(Debug, clap::Args)]
pub struct DoctorArgs {}

impl Run for DoctorArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let start = std::time::Instant::now();
    let project = ctx.try_project()?;
    let (probes, what) = match &project {
      Some(project) => (
        project_probes(project)?,
        format!("tools for {}", project.name()),
      ),
      None => (
        generic_probes(),
        "tools (outside a project, everything is optional)".to_owned(),
      ),
    };
    ctx.shell.status("Checking", what)?;
    let results: Vec<_> = probes.par_iter().map(Probe::run).collect();
    let name_width = probes
      .iter()
      .map(|p| p.name.len())
      .max()
      .unwrap_or_default();
    let found_width = results
      .iter()
      .map(|r| r.as_ref().map_or(0, String::len))
      .max()
      .unwrap_or_default();
    let mut missing = 0;
    for (probe, result) in probes.iter().zip(&results) {
      let (name, purpose, need) = (&probe.name, &probe.purpose, probe.need);
      match result {
        Ok(version) => ctx.shell.status_with_style(
          "ok",
          format_args!("{name:<name_width$}  {version:<found_width$}  {purpose}"),
          &style::HEADER,
        )?,
        Err(reason) => {
          let (status, style) = match need {
            Need::Required => ("MISSING", &style::ERROR),
            Need::Recommended => ("missing", &style::WARN),
            Need::Optional => ("missing", &style::DIM),
          };
          missing += usize::from(need == Need::Required);
          ctx.shell.status_with_style(
            status,
            format_args!("{name:<name_width$}  {purpose} ({need}): {reason}"),
            style,
          )?;
        }
      }
    }
    if missing > 0 {
      let verb = if missing == 1 { "is" } else { "are" };
      return Err(
        Failure::new(
          format!("{} {verb} missing", plural(missing, "required tool")),
          1,
        )
        .into(),
      );
    }
    ctx.shell.finished("doctor", start.elapsed())?;
    Ok(())
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
enum Need {
  #[display("required")]
  Required,
  #[display("recommended")]
  Recommended,
  #[display("optional")]
  Optional,
}

impl Need {
  fn when(required: bool) -> Self {
    if required {
      Self::Required
    } else {
      Self::Optional
    }
  }
}

#[derive(Debug, Clone)]
struct Probe {
  name: String,
  command: ProcessBuilder,
  need: Need,
  purpose: String,
}

impl Probe {
  fn new(name: &str, command: ProcessBuilder, need: Need, purpose: &str) -> Self {
    Self {
      name: name.to_owned(),
      command,
      need,
      purpose: purpose.to_owned(),
    }
  }

  fn system(name: &str, need: Need, purpose: &str) -> Self {
    Self::new(
      name,
      ProcessBuilder::new(name).arg("--version"),
      need,
      purpose,
    )
  }

  fn cargo(name: &str, need: Need, purpose: &str) -> Self {
    Self::new(
      &format!("cargo-{name}"),
      ProcessBuilder::new("cargo").args([name, "--version"]),
      need,
      purpose,
    )
  }

  fn nightly_rustfmt(need: Need) -> Self {
    let command = ProcessBuilder::new("rustfmt").args(["+nightly", "--version"]);
    Self::new("rustfmt +nightly", command, need, "nightly formatting")
  }

  fn run(&self) -> Result<String, String> {
    if which::which(self.command.get_program()).is_err() {
      return Err("not found".to_owned());
    }
    let output = self.command.output_unchecked().map_err(|e| e.to_string())?;
    let (stdout, stderr) = (
      String::from_utf8_lossy(&output.stdout),
      String::from_utf8_lossy(&output.stderr),
    );
    if output.status.success() {
      return Ok(
        version(&stdout)
          .or_else(|| version(&stderr))
          .unwrap_or("found")
          .to_owned(),
      );
    }
    let reason = stderr
      .lines()
      .chain(stdout.lines())
      .map(str::trim)
      .find(|l| !l.is_empty())
      .unwrap_or("failed");
    Err(reason.trim_start_matches("error: ").to_owned())
  }
}

fn version(text: &str) -> Option<&str> {
  lazy_regex::regex_find!(
    r"\b\d+\.\d+(?:\.\d+)?(?:[-+~][0-9A-Za-z.+~-]*[0-9A-Za-z])?\b",
    text
  )
}

fn generic_probes() -> Vec<Probe> {
  let optional = Need::Optional;
  vec![
    Probe::system("git", Need::Recommended, "version control"),
    Probe::system("uv", optional, "runs pinned Python tools"),
    Probe::system("conan", optional, "C/C++ dependencies"),
    Probe::system("cmake", optional, "C/C++ builds"),
    Probe::system("ninja", optional, "fast CMake generator"),
    Probe::system("cargo", optional, "Rust builds"),
    Probe::nightly_rustfmt(optional),
    Probe::cargo("nextest", optional, "Rust test runner"),
    Probe::cargo("deb", optional, "Debian packages of Rust crates"),
    Probe::system("makepkg", optional, "Arch packages"),
  ]
}

fn project_probes(project: &Project) -> Result<Vec<Probe>> {
  let m = &project.manifest;
  let conan = project.join("conanfile.py").is_file() || m.build.system == Some(BuildSystem::Conan);
  let tools = tool_probes(project);
  let pinned = tools
    .iter()
    .any(|(tool, probe)| probe.need == Need::Required && tool.version().is_some());
  let arch = m.package.arch.locate(&project.root).is_some();
  let distro = Distro::detect();
  let mut probes = vec![
    Probe::system("git", Need::Required, "version control"),
    Probe::system("uv", Need::when(pinned), "runs pinned Python tools"),
  ];
  if conan || m.has_c_family() {
    probes.push(Probe::system(
      "conan",
      Need::when(conan),
      "C/C++ dependencies",
    ));
    probes.push(Probe::system("cmake", Need::Required, "C/C++ builds"));
    probes.push(Probe::system(
      "ninja",
      Need::Recommended,
      "fast CMake generator",
    ));
  }
  if m.has(Language::Rust) {
    let nightly = Rustfmt::new(project)?.is_some_and(|r| r.channel == RustfmtChannel::Nightly);
    let nextest = m
      .test
      .suites
      .iter()
      .any(|s| s.runner == TestRunner::Nextest);
    probes.push(Probe::system("cargo", Need::Required, "Rust builds"));
    probes.push(Probe::nightly_rustfmt(Need::when(nightly)));
    probes.push(Probe::cargo(
      "nextest",
      if nextest {
        Need::Required
      } else {
        Need::Recommended
      },
      "Rust test runner",
    ));
    if m.package.deb.is_some() || distro == Distro::Debian {
      probes.push(Probe::cargo(
        "deb",
        Need::Optional,
        "Debian packages of Rust crates",
      ));
    }
  }
  if arch || distro == Distro::Arch {
    probes.push(Probe::system(
      "makepkg",
      if arch {
        Need::Recommended
      } else {
        Need::Optional
      },
      "Arch packages",
    ));
  }
  probes.extend(tools.into_iter().map(|(_, probe)| probe));
  Ok(probes)
}

fn tool_probes(project: &Project) -> Vec<(Tool, Probe)> {
  let m = &project.manifest;
  let mut wanted: Vec<(&str, Tool, Need, String)> = Kind::ALL
    .into_iter()
    .filter_map(|kind| ToolFormatter::new(project, kind))
    .map(|f| {
      (
        f.kind.tool(),
        f.tool,
        Need::Required,
        format!("{} formatting", f.kind.language()),
      )
    })
    .collect();
  if m.clang_tidy_enabled() {
    wanted.push((
      "clang-tidy",
      project.tool("clang-tidy"),
      Need::Required,
      "C/C++ lints".into(),
    ));
  }
  if m.check.qmllint.enabled {
    wanted.push((
      "qmllint",
      project.tool("qmllint"),
      Need::Required,
      "QML lints".into(),
    ));
  }
  for name in m.tools.names() {
    wanted.push((
      name,
      project.tool(name),
      Need::Recommended,
      "pinned in `[tools]`".into(),
    ));
  }
  let mut probes: Vec<(Tool, Probe)> = Vec::new();
  for (name, tool, need, purpose) in wanted {
    if !probes.iter().any(|(_, p)| p.name == name) {
      let probe = Probe::new(name, tool.command().arg("--version"), need, &purpose);
      probes.push((tool, probe));
    }
  }
  probes
}

#[derive(Debug, clap::Args)]
pub struct InfoArgs {
  /// Print a JSON object instead of text
  #[arg(long)]
  json: bool,
}

impl Run for InfoArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let host = HostInfo::detect();
    let project = ctx
      .try_project()?
      .map(|project| ProjectInfo::collect(&project))
      .transpose()?;
    let out = ctx.shell.out();
    if self.json {
      let info =
        serde_json::json!({ "wkit": env!("CARGO_PKG_VERSION"), "host": host, "project": project });
      writeln!(out, "{}", serde_json::to_string_pretty(&info)?)?;
      return Ok(());
    }
    let mut sections = vec![host.fields()];
    sections.extend(project.as_ref().map(ProjectInfo::fields));
    let width = sections
      .iter()
      .flatten()
      .map(|(key, _)| key.len() + 1)
      .max()
      .unwrap_or_default();
    writeln!(out, "wkit {}", env!("CARGO_PKG_VERSION"))?;
    for section in sections {
      writeln!(out)?;
      for (key, value) in section {
        writeln!(out, "{:<width$} {value}", format!("{key}:"))?;
      }
    }
    Ok(())
  }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct HostInfo {
  triple: String,
  os: String,
  family: String,
  kernel: Option<String>,
  cpus: usize,
}

impl HostInfo {
  fn detect() -> Self {
    let os = Distro::OS_RELEASE
      .iter()
      .find_map(|path| std::fs::read_to_string(path).ok())
      .and_then(|text| pretty_name(&text))
      .unwrap_or_else(|| std::env::consts::OS.to_owned());
    Self {
      triple: triple(),
      os,
      family: Distro::detect().to_string(),
      kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|k| k.trim().to_owned()),
      cpus: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    }
  }

  fn fields(&self) -> Vec<(&'static str, String)> {
    let mut fields = vec![
      ("host", self.triple.clone()),
      ("os", format!("{} ({})", self.os, self.family)),
    ];
    fields.extend(self.kernel.clone().map(|kernel| ("kernel", kernel)));
    fields.push(("cpus", self.cpus.to_string()));
    fields
  }
}

fn triple() -> String {
  let vendor = if cfg!(target_vendor = "apple") {
    "apple"
  } else if cfg!(target_vendor = "pc") {
    "pc"
  } else {
    "unknown"
  };
  let os = match std::env::consts::OS {
    "macos" => "darwin",
    other => other,
  };
  let env = if cfg!(target_env = "gnu") {
    "-gnu"
  } else if cfg!(target_env = "musl") {
    "-musl"
  } else if cfg!(target_env = "msvc") {
    "-msvc"
  } else {
    ""
  };
  format!("{}-{vendor}-{os}{env}", std::env::consts::ARCH)
}

fn pretty_name(os_release: &str) -> Option<String> {
  os_release
    .lines()
    .filter_map(|line| line.split_once('='))
    .find(|(key, _)| key.trim() == "PRETTY_NAME")
    .map(|(_, value)| value.trim().trim_matches(['"', '\'']).to_owned())
    .filter(|name| !name.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct ProjectInfo {
  name: String,
  root: PathBuf,
  version: Option<String>,
  languages: Vec<String>,
  build: Option<String>,
  bins: Vec<String>,
  tests: Vec<String>,
  formatters: Vec<String>,
  checks: Vec<String>,
  packages: Vec<String>,
}

impl ProjectInfo {
  fn collect(project: &Project) -> Result<Self> {
    let m = &project.manifest;
    let version = match &m.version {
      Some(section) => Some(
        VersionSet::load(&project.root, section)?
          .current()?
          .render(Flavor::Full),
      ),
      None => None,
    };
    let packages = [
      ("deb", m.package.deb.is_some()),
      ("arch", m.package.arch.locate(&project.root).is_some()),
    ];
    Ok(Self {
      name: project.name(),
      root: project.root.clone(),
      version,
      languages: m.languages().iter().map(ToString::to_string).collect(),
      build: Builder::new(project, &BuildOptions::default())
        .system()
        .map(|s| s.to_string()),
      bins: m.bins.iter().map(|b| b.name.clone()).collect(),
      tests: m.test.suites.iter().map(|s| s.runner.to_string()).collect(),
      formatters: Formatter::new(project)?.names(),
      checks: Checker::plan(project, &[], &[])?.names(),
      packages: packages
        .iter()
        .filter(|(_, has)| *has)
        .map(|(name, _)| (*name).to_owned())
        .collect(),
    })
  }

  fn fields(&self) -> Vec<(&'static str, String)> {
    let list = |items: &[String]| {
      if items.is_empty() {
        "none".to_owned()
      } else {
        items.join(", ")
      }
    };
    let or_none = |item: &Option<String>| item.clone().unwrap_or_else(|| "none".to_owned());
    vec![
      ("project", self.name.clone()),
      ("root", self.root.display().to_string()),
      ("version", or_none(&self.version)),
      ("languages", list(&self.languages)),
      ("build", or_none(&self.build)),
      ("bins", list(&self.bins)),
      ("tests", list(&self.tests)),
      ("formatters", list(&self.formatters)),
      ("checks", list(&self.checks)),
      ("packages", list(&self.packages)),
    ]
  }
}

#[derive(Debug, clap::Args)]
pub struct SelfUpdateArgs {
  /// Install this exact version instead of the newest one
  #[arg(long, value_name = "VERSION")]
  version: Option<semver::Version>,
}

impl Run for SelfUpdateArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let current = Context::version();
    let requirement = match ctx.try_project() {
      Ok(project) => project.and_then(|p| p.manifest.project.wkit),
      Err(e) => {
        ctx
          .shell
          .warn(format_args!("ignoring `project.wkit`: {e}"))?;
        None
      }
    };
    if let (Some(pinned), Some(requirement)) = (&self.version, &requirement)
      && !requirement.matches(pinned)
    {
      ctx.shell.warn(format_args!(
        "wkit {pinned} does not satisfy `project.wkit = \"{requirement}\"`"
      ))?;
    }
    ctx.shell.status("Checking", "crates.io for wkit updates")?;
    let target =
      CrateIndex::fetch()?.select(&current, requirement.as_ref(), self.version.as_ref())?;
    if self.version.is_none() && target <= current {
      ctx
        .shell
        .status("Fresh", format_args!("wkit v{current} is up to date"))?;
      return Ok(());
    }
    if target == current {
      ctx.shell.status(
        "Fresh",
        format_args!("wkit v{current} is already installed"),
      )?;
      return Ok(());
    }
    ctx
      .shell
      .status("Updating", format_args!("wkit v{current} -> v{target}"))?;
    let install = if which::which("cargo-binstall").is_ok() {
      ProcessBuilder::new("cargo")
        .args(["binstall", "--no-confirm", "--force"])
        .arg(format!("wkit@={target}"))
    } else {
      ProcessBuilder::new("cargo")
        .args(["install", "wkit", "--locked", "--force", "--version"])
        .arg(format!("={target}"))
    };
    ctx.shell.verbose(|s| s.status("Running", &install))?;
    let parked = park_running_exe();
    let status = install.status_unchecked()?;
    if !status.success() {
      if let Some((exe, old)) = parked {
        let _ = std::fs::rename(old, exe);
      }
      let code = status.code().unwrap_or(Failure::INTERNAL);
      return Err(Failure::new(format!("`{install}` failed ({status})"), code).into());
    }
    ctx
      .shell
      .status("Updated", format_args!("wkit to v{target}"))?;
    Ok(())
  }
}

fn park_running_exe() -> Option<(PathBuf, PathBuf)> {
  if !cfg!(windows) {
    return None;
  }
  let exe = std::env::current_exe().ok()?;
  let old = exe.with_extension("exe.old");
  let _ = std::fs::remove_file(&old);
  std::fs::rename(&exe, &old).ok()?;
  Some((exe, old))
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
struct IndexEntry {
  vers: semver::Version,
  #[serde(default)]
  yanked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CrateIndex {
  entries: Vec<IndexEntry>,
}

impl CrateIndex {
  const URL: &str = "https://index.crates.io/wk/it/wkit";

  fn fetch() -> Result<Self> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
      .timeout_global(Some(std::time::Duration::from_secs(30)))
      .user_agent(concat!(
        "wkit/",
        env!("CARGO_PKG_VERSION"),
        " (+",
        env!("CARGO_PKG_REPOSITORY"),
        ")"
      ))
      .build()
      .into();
    let context = || format!("failed to fetch `{}`", Self::URL);
    let mut response = match agent.get(Self::URL).call() {
      Ok(response) => response,
      Err(ureq::Error::StatusCode(404)) => anyhow::bail!("wkit is not published on crates.io"),
      Err(e) => return Err(anyhow::Error::from(e).context(context())),
    };
    Self::parse(&response.body_mut().read_to_string().with_context(context)?)
  }

  fn parse(text: &str) -> Result<Self> {
    let entries = text
      .lines()
      .filter(|line| !line.trim().is_empty())
      .map(serde_json::from_str)
      .collect::<Result<_, _>>()
      .context("crates.io returned a malformed index entry")?;
    Ok(Self { entries })
  }

  fn select(
    &self,
    current: &semver::Version,
    requirement: Option<&semver::VersionReq>,
    pinned: Option<&semver::Version>,
  ) -> Result<semver::Version> {
    if let Some(pinned) = pinned {
      return match self.entries.iter().find(|e| e.vers == *pinned) {
        Some(entry) if entry.yanked => anyhow::bail!("wkit {pinned} was yanked"),
        Some(entry) => Ok(entry.vers.clone()),
        None => anyhow::bail!("wkit {pinned} is not published on crates.io"),
      };
    }
    self
      .entries
      .iter()
      .filter(|e| !e.yanked && (e.vers.pre.is_empty() || !current.pre.is_empty()))
      .filter(|e| requirement.is_none_or(|r| r.matches(&e.vers)))
      .map(|e| &e.vers)
      .max()
      .cloned()
      .ok_or_else(|| match requirement {
        Some(r) => anyhow::anyhow!("no published wkit version matches `{r}` from `project.wkit`"),
        None => anyhow::anyhow!("no published wkit versions found"),
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn versions_are_picked_out_of_banner_lines() {
    for (banner, expected) in [
      ("git version 2.51.0\n", "2.51.0"),
      ("Conan version 2.21.0", "2.21.0"),
      ("cmake version 4.1.2\n\nCMake suite maintained", "4.1.2"),
      (
        "rustfmt 1.8.0-nightly (6d6a08cf59 2025-09-04)",
        "1.8.0-nightly",
      ),
      ("qmlformat 6.12.0", "6.12.0"),
      ("1.13.1\n", "1.13.1"),
      ("cargo-deb 3.6.0", "3.6.0"),
    ] {
      assert_eq!(version(banner), Some(expected), "{banner}");
    }
    assert_eq!(version("no digits"), None);
  }

  #[test]
  fn missing_and_failing_programs_are_reported() {
    let missing = Probe::system("wkit-surely-missing-tool", Need::Optional, "");
    assert_eq!(missing.run(), Err("not found".into()));
    let failing = Probe::new(
      "sh",
      ProcessBuilder::shell("echo 'error: no such command' >&2; exit 101"),
      Need::Required,
      "",
    );
    assert_eq!(failing.run(), Err("no such command".into()));
    let found = Probe::new(
      "sh",
      ProcessBuilder::shell("echo tool 1.2.3"),
      Need::Required,
      "",
    );
    assert_eq!(found.run(), Ok("1.2.3".into()));
  }

  #[test]
  fn the_triple_names_this_build_host() {
    let triple = triple();
    assert!(triple.starts_with(std::env::consts::ARCH), "{triple}");
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
      assert!(triple.ends_with("-unknown-linux-gnu"), "{triple}");
    }
  }

  #[test]
  fn pretty_names_come_from_os_release() {
    assert_eq!(
      pretty_name("NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n").as_deref(),
      Some("Arch Linux")
    );
    assert_eq!(pretty_name("ID=arch\n"), None);
  }

  fn index() -> CrateIndex {
    let entry = |vers: &str, yanked: bool| {
      format!(
        r#"{{"name":"wkit","vers":"{vers}","deps":[],"cksum":"x","features":{{}},"yanked":{yanked}}}"#
      )
    };
    let lines = [
      entry("0.1.0", false),
      entry("0.2.0", false),
      entry("0.3.0-rc.1", false),
      entry("0.2.1", true),
    ];
    CrateIndex::parse(&lines.join("\n")).unwrap()
  }

  fn v(s: &str) -> semver::Version {
    semver::Version::parse(s).unwrap()
  }

  #[test]
  fn picks_the_newest_stable_unyanked_release() {
    assert_eq!(index().select(&v("0.1.0"), None, None).unwrap(), v("0.2.0"));
    assert_eq!(
      index().select(&v("0.3.0-alpha.1"), None, None).unwrap(),
      v("0.3.0-rc.1")
    );
  }

  #[test]
  fn honors_requirements_and_pins() {
    let req = semver::VersionReq::parse("~0.1").unwrap();
    assert_eq!(
      index().select(&v("0.1.0"), Some(&req), None).unwrap(),
      v("0.1.0")
    );
    let req = semver::VersionReq::parse(">=1").unwrap();
    assert!(index().select(&v("0.1.0"), Some(&req), None).is_err());
    assert_eq!(
      index()
        .select(&v("0.2.0"), None, Some(&v("0.1.0")))
        .unwrap(),
      v("0.1.0")
    );
    assert!(
      index()
        .select(&v("0.2.0"), None, Some(&v("0.2.1")))
        .is_err()
    );
    assert!(
      index()
        .select(&v("0.2.0"), None, Some(&v("9.9.9")))
        .is_err()
    );
  }
}
