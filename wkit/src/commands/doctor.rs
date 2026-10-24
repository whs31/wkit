mod need;
mod probe;

use wkit_check::Qmllint;
use wkit_core::{ProcessBuilder, detect::RustfmtChannel, shell::style};
use wkit_fmt::{ExtraFormatter, ExtraKind, Rustfmt};
use wkit_manifest::{Project, TestRunner};
use wkit_package::Distro;

pub use self::{
  need::Need,
  probe::{Found, Probe},
};
use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct DoctorArgs {}

impl DoctorArgs {
  pub fn run(ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let project = ctx.try_project()?;
    let probes = match &project {
      Some(project) => Self::project_probes(project)?,
      None => Self::generic_probes(),
    };
    ctx.shell.status(
      "Checking",
      match &project {
        Some(project) => format!("tools for {}", project.name()),
        None => "tools (outside a project, everything is optional)".to_owned(),
      },
    )?;
    let results: Vec<_> = std::thread::scope(|scope| {
      let handles: Vec<_> = probes.iter().map(|p| scope.spawn(|| p.run())).collect();
      handles
        .into_iter()
        .map(|h| {
          h.join()
            .unwrap_or_else(|_| Found::Missing("probe panicked".into()))
        })
        .collect()
    });
    let name_width = probes
      .iter()
      .map(|p| p.name.len())
      .max()
      .unwrap_or_default();
    let found_width = results
      .iter()
      .map(|r| match r {
        Found::Version(v) => v.len(),
        Found::Missing(_) => 0,
      })
      .max()
      .unwrap_or_default();
    let mut missing = 0;
    for (probe, result) in probes.iter().zip(&results) {
      match result {
        Found::Version(version) => ctx.shell.status_with_style(
          "ok",
          format_args!(
            "{:<name_width$}  {version:<found_width$}  {}",
            probe.name, probe.purpose
          ),
          &style::HEADER,
        )?,
        Found::Missing(reason) => {
          let (status, style) = match probe.need {
            Need::Required => {
              missing += 1;
              ("MISSING", &style::ERROR)
            }
            Need::Recommended => ("missing", &style::WARN),
            Need::Optional => ("missing", &style::DIM),
          };
          ctx.shell.status_with_style(
            status,
            format_args!(
              "{:<name_width$}  {} ({}): {reason}",
              probe.name,
              probe.purpose,
              probe.need.as_str()
            ),
            style,
          )?;
        }
      }
    }
    if missing > 0 {
      return Err(
        Failure::new(
          format!(
            "{missing} required tool{} missing",
            if missing == 1 { " is" } else { "s are" }
          ),
          1,
        )
        .into(),
      );
    }
    ctx.shell.finished("doctor", start.elapsed())?;
    Ok(())
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
      Self::nightly_rustfmt(optional),
      Self::cargo_tool("nextest", optional, "Rust test runner"),
      Self::cargo_tool("deb", optional, "Debian packages of Rust crates"),
      Probe::system("makepkg", optional, "Arch packages"),
    ]
  }

  fn project_probes(project: &Project) -> anyhow::Result<Vec<Probe>> {
    let m = &project.manifest;
    let c_family = m.has_c_family();
    let conan = project.join("conanfile.py").is_file()
      || m.build.system == Some(wkit_core::detect::BuildSystem::Conan);
    let tools = Self::tool_probes(project);
    let pinned = tools
      .iter()
      .any(|(tool, probe)| probe.need == Need::Required && tool.version().is_some());
    let arch = m.package.arch.locate(&project.root).is_some();
    let distro = Distro::detect();
    let mut probes = vec![
      Probe::system("git", Need::Required, "version control"),
      Probe::system("uv", Need::when(pinned), "runs pinned Python tools"),
    ];
    if conan || c_family {
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
    if m.has(wkit_core::Language::Rust) {
      let nightly = Rustfmt::new(project)?.is_some_and(|r| r.channel() == RustfmtChannel::Nightly);
      let nextest = m
        .test
        .suites
        .iter()
        .any(|s| s.runner == TestRunner::Nextest);
      probes.push(Probe::system("cargo", Need::Required, "Rust builds"));
      probes.push(Self::nightly_rustfmt(Need::when(nightly)));
      probes.push(Self::cargo_tool(
        "nextest",
        if nextest {
          Need::Required
        } else {
          Need::Recommended
        },
        "Rust test runner",
      ));
      if m.package.deb.is_some() || distro == Distro::Debian {
        probes.push(Self::cargo_tool(
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

  fn tool_probes(project: &Project) -> Vec<(wkit_core::tool::Tool, Probe)> {
    let m = &project.manifest;
    let mut wanted: Vec<(&str, wkit_core::tool::Tool, Need, String)> = Vec::new();
    if m.cpp_format_enabled() {
      wanted.push((
        "clang-format",
        project.tool("clang-format"),
        Need::Required,
        "C/C++ formatting".into(),
      ));
    }
    if m.clang_tidy_enabled() {
      wanted.push((
        "clang-tidy",
        project.tool("clang-tidy"),
        Need::Required,
        "C/C++ lints".into(),
      ));
    }
    for kind in ExtraKind::ALL {
      if let Some(formatter) = ExtraFormatter::new(project, kind) {
        let purpose = format!("{} formatting", kind.language());
        wanted.push((
          kind.tool(),
          formatter.tool().clone(),
          Need::Required,
          purpose,
        ));
      }
    }
    if m.check.qmllint.enabled {
      wanted.push((
        Qmllint::TOOL,
        project.tool(Qmllint::TOOL),
        Need::Required,
        "QML lints".into(),
      ));
    }
    for (name, _) in m.tools.iter() {
      wanted.push((
        name,
        project.tool(name),
        Need::Recommended,
        "pinned in `[tools]`".into(),
      ));
    }
    let mut probes: Vec<(wkit_core::tool::Tool, Probe)> = Vec::new();
    for (name, tool, need, purpose) in wanted {
      if !probes.iter().any(|(_, p)| p.name == name) {
        let probe = Probe::new(name, tool.command().arg("--version"), need, &purpose);
        probes.push((tool, probe));
      }
    }
    probes
  }

  fn nightly_rustfmt(need: Need) -> Probe {
    Probe::new(
      "rustfmt +nightly",
      ProcessBuilder::new("rustfmt").args(["+nightly", "--version"]),
      need,
      "nightly formatting",
    )
  }

  fn cargo_tool(name: &str, need: Need, purpose: &str) -> Probe {
    Probe::new(
      &format!("cargo-{name}"),
      ProcessBuilder::new("cargo").args([name, "--version"]),
      need,
      purpose,
    )
  }
}
