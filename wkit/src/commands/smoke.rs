mod smoke_run;

use wkit_check::Status;

pub use self::smoke_run::SmokeRun;
use super::build_selection::BuildSelection;
use crate::{context::Context, failure::Failure, launcher::Launcher};

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

impl SmokeArgs {
  pub const TAIL: usize = 20;

  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    anyhow::ensure!(self.runs > 0, "`--runs` must be at least 1");
    let timeout = Self::seconds("--timeout", self.timeout)?;
    let interval = Self::seconds("--interval", self.interval)?;
    let project = ctx.project()?;
    let options = self.selection.clone().options();
    let launcher = Launcher::new(&project, &options, self.bin.as_deref())?;
    let mut process = launcher.build(ctx.shell, false)?.args(&self.args);
    if process.get_cwd().is_none() {
      process = process.cwd(&ctx.cwd);
    }
    let logs = project.join(if project.manifest.has_c_family() {
      "build"
    } else {
      "target"
    });
    let logs = logs.join("smoke");
    if logs.is_dir() {
      std::fs::remove_dir_all(&logs)
        .map_err(|e| anyhow::anyhow!("cannot clear `{}`: {e}", logs.display()))?;
    }
    std::fs::create_dir_all(&logs)
      .map_err(|e| anyhow::anyhow!("cannot create `{}`: {e}", logs.display()))?;
    let start = std::time::Instant::now();
    ctx.shell.status(
      "Launching",
      format_args!(
        "`{}` {} (timeout {}s)",
        launcher.display(&process),
        if self.runs == 1 {
          "once".to_owned()
        } else {
          format!("{} times", self.runs)
        },
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
        failed += 1;
        Status::Failed
      };
      ctx.shell.status_with_style(
        status,
        format_args!("run {index}  {}", run.describe()),
        status.style(),
      )?;
      if status == Status::Failed {
        Self::tail(ctx, &log)?;
      }
    }
    if failed > 0 {
      return Err(
        Failure::new(
          format!(
            "{failed} of {} runs failed; logs in `{}`",
            self.runs,
            logs.strip_prefix(&project.root).unwrap_or(&logs).display()
          ),
          1,
        )
        .into(),
      );
    }
    ctx.shell.finished("smoke test", start.elapsed())?;
    Ok(())
  }

  fn seconds(flag: &str, value: f64) -> anyhow::Result<std::time::Duration> {
    std::time::Duration::try_from_secs_f64(value)
      .map_err(|_| anyhow::anyhow!("`{flag}` must be a non-negative number of seconds"))
  }

  fn tail(ctx: &mut Context<'_>, log: &std::path::Path) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<_> = text.lines().collect();
    let err = ctx.shell.err();
    for line in &lines[lines.len().saturating_sub(Self::TAIL)..] {
      writeln!(err, "  {line}")?;
    }
    Ok(())
  }
}
