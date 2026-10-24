use std::io::Write;

use wkit_core::{ProcessBuilder, Shell, shell::Verbosity};

use crate::{Error, Result};

#[derive(Debug)]
pub struct ToolRun<'s> {
  shell: &'s mut Shell,
  log: std::path::PathBuf,
  file: std::fs::File,
}

impl<'s> ToolRun<'s> {
  pub const TAIL: usize = 30;

  pub fn new(shell: &'s mut Shell, log: std::path::PathBuf) -> Result<Self> {
    if let Some(parent) = log.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(&log).map_err(|source| wkit_core::Error::Write {
      path: log.clone(),
      source,
    })?;
    Ok(Self { shell, log, file })
  }

  pub fn shell(&mut self) -> &mut Shell {
    self.shell
  }

  pub fn run(
    &mut self,
    process: &ProcessBuilder,
    status: &str,
    mut translate: impl FnMut(&str) -> Option<(&'static str, String)>,
  ) -> Result<()> {
    let Self { shell, log, file } = self;
    shell.verbose(|s| s.status("Running", process))?;
    writeln!(file, "$ {process}")?;
    let verbose = shell.verbosity() == Verbosity::Verbose;
    let bar = (!verbose).then(|| shell.transient(status));
    let mut tail = std::collections::VecDeque::with_capacity(Self::TAIL);
    let mut failure = None;
    let exit = process.stream(|line| {
      let mut handle = || -> std::io::Result<()> {
        writeln!(file, "{line}")?;
        if verbose {
          return shell.passthrough(line);
        }
        if tail.len() == Self::TAIL {
          tail.pop_front();
        }
        tail.push_back(line.to_owned());
        if let Some((status, message)) = translate(line) {
          return shell.status(status, message);
        }
        if let Some(bar) = &bar {
          bar.set_message(line.trim().to_owned());
        }
        Ok(())
      };
      if let Err(e) = handle() {
        failure.get_or_insert(e);
      }
    })?;
    if let Some(bar) = bar {
      bar.finish_and_clear();
    }
    file.flush()?;
    if let Some(e) = failure {
      return Err(e.into());
    }
    if exit.success() {
      return Ok(());
    }
    for line in tail {
      shell.passthrough(line)?;
    }
    Err(Error::ToolFailed {
      tool: process.get_program().to_string_lossy().into_owned(),
      status: exit,
      log: log.clone(),
    })
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;

  #[test]
  fn failures_show_the_tail_and_keep_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("logs/tool.log");
    let mut shell = Shell::from_write(Box::new(std::io::sink()));
    let mut run = ToolRun::new(&mut shell, log.clone()).unwrap();
    let mut seen = Vec::new();
    run
      .run(
        &ProcessBuilder::shell("echo '==> one'; echo two"),
        "Running",
        |line| {
          seen.push(line.to_owned());
          line
            .strip_prefix("==> ")
            .map(|rest| ("Step", rest.to_owned()))
        },
      )
      .unwrap();
    assert_eq!(seen, ["==> one", "two"]);
    let err = run
      .run(
        &ProcessBuilder::shell("echo boom; exit 3"),
        "Running",
        |_| None,
      )
      .unwrap_err();
    assert_eq!(err.exit_code(), Some(3));
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("two\n") && text.contains("boom\n"), "{text}");
  }
}
