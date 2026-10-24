use wkit_fmt::{Formatter, Mode};

use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct FmtArgs {
  /// Report unformatted files instead of formatting them
  #[arg(long)]
  check: bool,

  /// Number of parallel formatter jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,
}

impl FmtArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let formatter = Formatter::new(&project)?;
    if formatter.is_empty() {
      ctx
        .shell
        .warn("nothing to format: no formatter in `[format]` is enabled")?;
      return Ok(());
    }
    let start = std::time::Instant::now();
    let mode = if self.check { Mode::Check } else { Mode::Write };
    let jobs = self
      .jobs
      .or_else(|| std::thread::available_parallelism().ok())
      .map_or(1, std::num::NonZeroUsize::get);
    let files = formatter.run(ctx.shell, mode, jobs)?;
    match mode {
      Mode::Write => {
        for file in &files {
          ctx.shell.status("Formatted", file.display())?;
        }
        ctx.shell.finished("formatting", start.elapsed())?;
        Ok(())
      }
      Mode::Check if files.is_empty() => {
        ctx.shell.finished("format check", start.elapsed())?;
        Ok(())
      }
      Mode::Check => {
        let list: Vec<_> = files.iter().map(|f| f.display().to_string()).collect();
        Err(
          Failure::new(
            format!(
              "{} need{} formatting; run `wkit fmt`:\n  {}",
              Self::count(files.len()),
              if files.len() == 1 { "s" } else { "" },
              list.join("\n  ")
            ),
            1,
          )
          .into(),
        )
      }
    }
  }

  fn count(n: usize) -> String {
    if n == 1 {
      "1 file".to_owned()
    } else {
      format!("{n} files")
    }
  }
}
