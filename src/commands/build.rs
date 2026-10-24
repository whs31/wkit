use std::{num::NonZeroUsize, path::PathBuf};

use super::{Run, jobs};
use crate::{
  Failure, ProcessBuilder, Result,
  build::{BuildOptions, Builder, Conan, RemoteSync},
  cli::Context,
  manifest::BuildType,
  shell::{ColorChoice, plural},
  testing::{TestOptions, Tester},
};

#[derive(Debug, Clone, Default, clap::Args)]
pub struct BuildSelection {
  /// Build in release mode, same as `--build-type Release`
  #[arg(short, long, conflicts_with = "build_type")]
  release: bool,

  /// Build type [default: from the Conan profile, else Debug]
  #[arg(long, value_name = "TYPE")]
  build_type: Option<BuildType>,

  /// Conan host profile [default: `build.conan.profile-host`, then `default`]
  #[arg(long, value_name = "NAME")]
  profile: Option<String>,

  /// Use only packages that are already available locally
  #[arg(long)]
  offline: bool,
}

impl BuildSelection {
  pub fn options(self) -> BuildOptions {
    BuildOptions {
      build_type: self
        .build_type
        .or(self.release.then_some(BuildType::Release)),
      profile: self.profile,
      offline: self.offline,
      ..BuildOptions::default()
    }
  }
}

#[derive(Debug, clap::Args)]
pub struct BuildArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Also build the tests
  #[arg(short, long)]
  tests: bool,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<NonZeroUsize>,

  /// Arguments passed to the underlying build tool
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl Run for BuildArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      tests: self.tests,
      jobs: jobs(self.jobs),
      extra: self.args,
      ..self.selection.options()
    };
    Builder::new(&project, &options).run(ctx.shell)?;
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct ConfigureArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Configure with the tests enabled
  #[arg(short, long)]
  tests: bool,

  /// Arguments passed to `conan install`, or to `cmake` without Conan
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl Run for ConfigureArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      tests: self.tests,
      extra: self.args,
      ..self.selection.options()
    };
    if Builder::new(&project, &options)
      .configure(ctx.shell)?
      .is_none()
    {
      ctx
        .shell
        .warn("nothing to configure: the project has no Conan or CMake build")?;
    }
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct CreateArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<NonZeroUsize>,

  /// Arguments passed to `conan create`
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl Run for CreateArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: jobs(self.jobs),
      extra: self.args,
      ..self.selection.options()
    };
    Builder::new(&project, &options).create(ctx.shell)?;
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct GraphArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Where to write the HTML graph
  #[arg(short, long, value_name = "PATH", default_value = "docs/graph.html")]
  output: PathBuf,

  /// Open the graph in the default browser
  #[arg(long)]
  open: bool,
}

impl Run for GraphArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    anyhow::ensure!(
      project.join("conanfile.py").is_file(),
      "`wkit graph` needs a conanfile.py at the project root"
    );
    let options = self.selection.options();
    let build_type = Builder::new(&project, &options).build_type()?;
    let output = project.join(&self.output);
    if let Some(parent) = output.parent() {
      fs_err::create_dir_all(parent)?;
    }
    Conan::new(&project, &options).graph(ctx.shell, build_type, &output)?;
    ctx.shell.status("Generated", self.output.display())?;
    if self.open {
      let opener = if cfg!(windows) {
        ProcessBuilder::new("cmd").args(["/C", "start", ""])
      } else if cfg!(target_os = "macos") {
        ProcessBuilder::new("open")
      } else {
        ProcessBuilder::new("xdg-open")
      };
      opener.arg(&output).status()?;
    }
    Ok(())
  }
}

subcommands! {
  ConanCommand {
    /// Upload the resolved dependency graph from the local cache to a remote
    Sync(SyncArgs),
  }
}

#[derive(Debug, clap::Args)]
pub struct SyncArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Remote to upload to [default: `conan.sync.remote`]
  #[arg(long, value_name = "NAME")]
  remote: Option<String>,

  /// Check what would be uploaded without uploading
  #[arg(long)]
  dry_run: bool,
}

impl Run for SyncArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let options = self.selection.options();
    let build_type = Builder::new(&project, &options).build_type()?;
    let sync = RemoteSync::new(&project, &options, self.remote, self.dry_run);
    let count = sync.run(ctx.shell, build_type)?;
    let what = format!(
      "{} with `{}`",
      plural(count, "dependency"),
      sync.remote().unwrap_or_default()
    );
    if self.dry_run {
      ctx.shell.status("Checked", what)?;
      ctx.shell.warn("nothing uploaded due to --dry-run")?;
    } else {
      ctx.shell.status("Synchronized", what)?;
    }
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct TestArgs {
  /// Write merged test reports to `DIR/junit.xml` [default: `test.junit-dir`]
  #[allow(clippy::option_option)]
  #[arg(long, value_name = "DIR", num_args = 0..=1)]
  junit: Option<Option<PathBuf>>,

  /// Run only this suite (repeatable)
  #[arg(long = "suite", value_name = "NAME")]
  suites: Vec<String>,

  #[command(flatten)]
  selection: BuildSelection,

  /// Arguments passed to every test runner
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl Run for TestArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let junit = self.junit.map(|dir| match dir {
      Some(dir) => ctx.cwd.join(dir),
      None => project.join(project.manifest.test.junit_dir()),
    });
    let suites = Tester::select(&project, &self.suites)?;
    let build_options = BuildOptions {
      tests: true,
      ..self.selection.options()
    };
    let builder = Builder::new(&project, &build_options);
    let built = if suites.iter().any(|s| s.runner.needs_build_dir()) {
      builder.run_native(ctx.shell)?
    } else {
      None
    };
    let options = TestOptions {
      junit,
      suites: self.suites,
      build_type: match &built {
        Some(outcome) => outcome.build_type,
        None => builder.build_type()?,
      },
      build_dir: built
        .as_ref()
        .and_then(|outcome| outcome.build_dir().map(ToOwned::to_owned)),
      extra: self.args,
      color: match ctx.shell.color_choice() {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::IsTerminal::is_terminal(&std::io::stdout()),
      },
    };
    let tester = Tester::plan(&project, &options)?;
    if tester.is_empty() {
      ctx.shell.warn("no test suites to run")?;
      return Ok(());
    }
    let start = std::time::Instant::now();
    let reports = tester.run(ctx.shell)?;
    let failed: Vec<_> = reports.iter().filter(|r| !r.success).collect();
    let Some(first) = failed.first() else {
      ctx.shell.finished("tests", start.elapsed())?;
      return Ok(());
    };
    let names: Vec<_> = failed.iter().map(|r| r.name.as_str()).collect();
    let code = first.code.filter(|&c| c != 0).unwrap_or(1);
    Err(Failure::new(format!("test suites failed: {}", names.join(", ")), code).into())
  }
}
