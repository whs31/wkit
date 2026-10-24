use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

use anyhow::Context as _;

use super::{Run, build::BuildSelection, exit_status};
use crate::{
  Failure, ProcessBuilder, Result, Shell,
  build::{BuildOptions, Builder},
  check::Status,
  cli::Context,
  manifest::{Bin, BuildType, Project},
};

#[derive(Debug, clap::Args)]
pub struct RunArgs {
  /// The `[[bin]]` or executable to run [default: `project.default-run`]
  #[arg(long, value_name = "NAME")]
  bin: Option<String>,

  #[command(flatten)]
  selection: BuildSelection,

  /// Arguments passed to the program
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl Run for RunArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let options = self.selection.options();
    let launcher = Launcher::new(&project, &options, self.bin.as_deref())?;
    let process = launcher.build(ctx.shell, true, &ctx.cwd)?;
    if process.get_program() != "cargo" {
      ctx
        .shell
        .status("Running", format_args!("`{}`", launcher.display(&process)))?;
    }
    exit_status(process.args(&self.args).status_unchecked()?)
  }
}

#[derive(Debug, clap::Args)]
pub struct SmokeArgs {
  /// The `[[bin]]` or executable to launch [default: `project.default-run`]
  #[arg(value_name = "BIN")]
  bin: Option<String>,

  /// Number of launches
  #[arg(long, value_name = "N", default_value_t = 5)]
  runs: u32,

  /// Seconds to wait between launches
  #[arg(long, value_name = "SECONDS", default_value_t = 0.0)]
  interval: f64,

  /// Seconds a launch may run; a program still alive by then passes and is stopped
  #[arg(long, value_name = "SECONDS", default_value_t = 5.0)]
  timeout: f64,

  /// Fail launches that are still running at the timeout
  #[arg(long)]
  expect_exit: bool,

  #[command(flatten)]
  selection: BuildSelection,

  /// Arguments passed to the program
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl Run for SmokeArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    const TAIL: usize = 20;
    let seconds = |flag: &str, value: f64| {
      Duration::try_from_secs_f64(value)
        .map_err(|_| anyhow::anyhow!("`{flag}` must be a non-negative number of seconds"))
    };
    anyhow::ensure!(self.runs > 0, "`--runs` must be at least 1");
    let timeout = seconds("--timeout", self.timeout)?;
    let interval = seconds("--interval", self.interval)?;
    let project = ctx.project()?;
    let options = self.selection.options();
    let launcher = Launcher::new(&project, &options, self.bin.as_deref())?;
    let process = launcher.build(ctx.shell, false, &ctx.cwd)?.args(&self.args);
    let logs = project
      .join(if project.manifest.has_c_family() {
        "build"
      } else {
        "target"
      })
      .join("smoke");
    if logs.is_dir() {
      fs_err::remove_dir_all(&logs)?;
    }
    fs_err::create_dir_all(&logs)?;
    let start = Instant::now();
    let times = if self.runs == 1 {
      "once".to_owned()
    } else {
      format!("{} times", self.runs)
    };
    ctx.shell.status(
      "Launching",
      format_args!(
        "`{}` {times} (timeout {}s)",
        launcher.display(&process),
        self.timeout
      ),
    )?;
    let mut failed = 0;
    for index in 1..=self.runs {
      if index > 1 && !interval.is_zero() {
        std::thread::sleep(interval);
      }
      let log = logs.join(format!("run-{index}.log"));
      let run = SmokeRun::execute(&process, &log, timeout)?;
      let status = if run.passed(self.expect_exit) {
        Status::Passed
      } else {
        Status::Failed
      };
      ctx.shell.status_with_style(
        status,
        format_args!("run {index}  {}", run.describe()),
        status.style(),
      )?;
      if status == Status::Failed {
        failed += 1;
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let lines: Vec<_> = text.lines().collect();
        for line in &lines[lines.len().saturating_sub(TAIL)..] {
          writeln!(ctx.shell.err(), "  {line}")?;
        }
      }
    }
    if failed > 0 {
      let shown = logs.strip_prefix(&project.root).unwrap_or(&logs).display();
      return Err(
        Failure::new(
          format!("{failed} of {} runs failed; logs in `{shown}`", self.runs),
          1,
        )
        .into(),
      );
    }
    ctx.shell.finished("smoke test", start.elapsed())?;
    Ok(())
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SmokeRun {
  Exited {
    status: std::process::ExitStatus,
    elapsed: Duration,
  },
  Alive {
    elapsed: Duration,
  },
}

impl SmokeRun {
  const POLL: Duration = Duration::from_millis(20);
  #[cfg(unix)]
  const GRACE: Duration = Duration::from_secs(2);

  fn execute(process: &ProcessBuilder, log: &Path, timeout: Duration) -> Result<Self> {
    let file =
      std::fs::File::create(log).with_context(|| format!("cannot create `{}`", log.display()))?;
    let mut command = process.command();
    command
      .stdin(std::process::Stdio::null())
      .stdout(file.try_clone()?)
      .stderr(file);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let start = Instant::now();
    let mut child = command
      .spawn()
      .with_context(|| format!("cannot start `{process}`"))?;
    if let Some(status) = Self::wait(&mut child, timeout)? {
      return Ok(Self::Exited {
        status,
        elapsed: start.elapsed(),
      });
    }
    let elapsed = start.elapsed();
    Self::stop(&mut child)?;
    Ok(Self::Alive { elapsed })
  }

  fn passed(&self, expect_exit: bool) -> bool {
    match self {
      Self::Exited { status, .. } => status.success(),
      Self::Alive { .. } => !expect_exit,
    }
  }

  fn describe(&self) -> String {
    match self {
      Self::Exited { status, elapsed } if status.success() => {
        format!("exited cleanly after {:.2}s", elapsed.as_secs_f64())
      }
      Self::Exited { status, elapsed } => format!("{status} after {:.2}s", elapsed.as_secs_f64()),
      Self::Alive { elapsed } => {
        format!("still running after {:.2}s, stopped", elapsed.as_secs_f64())
      }
    }
  }

  fn wait(
    child: &mut std::process::Child,
    timeout: Duration,
  ) -> Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
      if let Some(status) = child.try_wait()? {
        return Ok(Some(status));
      }
      let now = Instant::now();
      if now >= deadline {
        return Ok(None);
      }
      std::thread::sleep(Self::POLL.min(deadline - now));
    }
  }

  fn stop(child: &mut std::process::Child) -> Result<()> {
    #[cfg(unix)]
    {
      use rustix::process::{Pid, Signal, kill_process_group};
      let group = Pid::from_child(child);
      kill_process_group(group, Signal::TERM).ok();
      Self::wait(child, Self::GRACE)?;
      kill_process_group(group, Signal::KILL).ok();
    }
    child.kill().ok();
    child.wait()?;
    Ok(())
  }
}

struct Launcher<'p> {
  project: &'p Project,
  options: &'p BuildOptions,
  bin: Bin,
  named: bool,
}

impl<'p> Launcher<'p> {
  fn new(project: &'p Project, options: &'p BuildOptions, name: Option<&str>) -> Result<Self> {
    let manifest = &project.manifest;
    let bare = |name: &str| Bin {
      name: name.to_owned(),
      ..Bin::default()
    };
    let bin = match name {
      Some(name) => manifest.bin(name).cloned().unwrap_or_else(|| bare(name)),
      None => match manifest.default_bin() {
        Some(bin) => bin.clone(),
        None if !manifest.bins.is_empty() => {
          let names: Vec<_> = manifest.bins.iter().map(|b| b.name.as_str()).collect();
          anyhow::bail!(
            "several `[[bin]]` entries and no `project.default-run`; pass a bin name, one of: {}",
            names.join(", ")
          )
        }
        None => bare(&project.name()),
      },
    };
    let named = name.is_some() || manifest.bin(&bin.name).is_some();
    Ok(Self {
      project,
      options,
      bin,
      named,
    })
  }

  fn build(&self, shell: &mut Shell, through_cargo: bool, cwd: &Path) -> Result<ProcessBuilder> {
    let builder = Builder::new(self.project, self.options);
    let native = self.bin.path.is_some() || (self.bin.package.is_none() && builder.builds_native());
    let process = if native {
      let outcome = builder.run_native(shell)?;
      let build_type = match &outcome {
        Some(outcome) => outcome.build_type,
        None => builder.build_type()?,
      };
      ProcessBuilder::new(
        self.executable(build_type, outcome.as_ref().and_then(|o| o.build_dir()))?,
      )
    } else if builder.cargo_applies() {
      let build_type = self.options.build_type.unwrap_or(BuildType::Debug);
      if through_cargo {
        let bin = self.named.then_some(self.bin.name.as_str());
        builder.cargo_run_command(build_type, self.bin.package.as_deref(), bin)
      } else {
        builder.cargo_build(build_type, false)?;
        ProcessBuilder::new(self.cargo_executable(build_type)?)
      }
    } else {
      anyhow::bail!("nothing to run: the project has no Conan, CMake or Cargo build");
    };
    let mut process = process.args(&self.bin.args).cwd(
      self
        .bin
        .cwd
        .as_ref()
        .map_or_else(|| cwd.to_owned(), |dir| self.project.join(dir)),
    );
    for (key, value) in &self.bin.env {
      process = process.env(key, value);
    }
    Ok(process)
  }

  fn display(&self, process: &ProcessBuilder) -> String {
    let program = Path::new(process.get_program());
    program
      .strip_prefix(&self.project.root)
      .unwrap_or(program)
      .display()
      .to_string()
  }

  fn executable(&self, build_type: BuildType, build_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = self.bin.resolve_path(build_type) {
      let path = self.project.join(path);
      anyhow::ensure!(
        path.is_file(),
        "`{}` does not exist after the build; check `path` in `[[bin]]`",
        path.display()
      );
      return Ok(path);
    }
    let dir = build_dir.context("the build reported no build directory")?;
    crate::fs::find_executable(dir, &self.bin.name).with_context(|| {
      format!(
        "no executable named `{}` in `{}`; add a `[[bin]]` with a `path`",
        self.bin.name,
        dir.display()
      )
    })
  }

  fn cargo_executable(&self, build_type: BuildType) -> Result<PathBuf> {
    let output = ProcessBuilder::new("cargo")
      .cwd(&self.project.root)
      .args(["metadata", "--no-deps", "--format-version", "1"])
      .output()?;
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let target = metadata["target_directory"]
      .as_str()
      .map_or_else(|| self.project.join("target"), PathBuf::from);
    let dir = target.join(if build_type.is_optimized() {
      "release"
    } else {
      "debug"
    });
    let exe = dir.join(format!("{}{}", self.bin.name, std::env::consts::EXE_SUFFIX));
    anyhow::ensure!(
      exe.is_file(),
      "no binary named `{}` in `{}`; name one of the package's binaries",
      self.bin.name,
      dir.display()
    );
    Ok(exe)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn run(script: &str, timeout_ms: u64) -> (SmokeRun, String) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("run.log");
    let run = SmokeRun::execute(
      &ProcessBuilder::shell(script),
      &log,
      Duration::from_millis(timeout_ms),
    )
    .unwrap();
    (run, std::fs::read_to_string(log).unwrap())
  }

  #[test]
  fn clean_exits_pass_and_output_is_logged() {
    let (run, log) = run("echo out&& echo err>&2", 5000);
    assert!(run.passed(true), "{run:?}");
    assert_eq!(log.replace("\r\n", "\n"), "out\nerr\n");
  }

  #[test]
  fn failures_fail_and_survivors_pass_unless_an_exit_is_expected() {
    let (failed, _) = run("exit 3", 5000);
    assert!(!failed.passed(false));
    assert!(failed.describe().contains('3'), "{}", failed.describe());
    let (alive, _) = run("sleep 30", 100);
    assert!(matches!(alive, SmokeRun::Alive { .. }));
    assert!(alive.passed(false) && !alive.passed(true));
  }
}
