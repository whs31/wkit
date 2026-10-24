use wkit_core::ProcessBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmokeRun {
  Exited {
    status: std::process::ExitStatus,
    elapsed: std::time::Duration,
  },
  Alive {
    elapsed: std::time::Duration,
  },
}

impl SmokeRun {
  pub const POLL: std::time::Duration = std::time::Duration::from_millis(20);
  pub const GRACE: std::time::Duration = std::time::Duration::from_secs(2);

  pub fn execute(
    process: &ProcessBuilder,
    log: &std::path::Path,
    timeout: std::time::Duration,
  ) -> anyhow::Result<Self> {
    let file = std::fs::File::create(log)
      .map_err(|e| anyhow::anyhow!("cannot create `{}`: {e}", log.display()))?;
    let mut command = process.command();
    command
      .stdin(std::process::Stdio::null())
      .stdout(file.try_clone()?)
      .stderr(file);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let start = std::time::Instant::now();
    let mut child = command
      .spawn()
      .map_err(|e| anyhow::anyhow!("cannot start `{process}`: {e}"))?;
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

  pub fn passed(&self, expect_exit: bool) -> bool {
    match self {
      Self::Exited { status, .. } => status.success(),
      Self::Alive { .. } => !expect_exit,
    }
  }

  pub fn describe(&self) -> String {
    match self {
      Self::Exited { status, elapsed } if status.success() => {
        format!("exited cleanly after {:.2}s", elapsed.as_secs_f64())
      }
      Self::Exited { status, elapsed } => {
        format!("{status} after {:.2}s", elapsed.as_secs_f64())
      }
      Self::Alive { elapsed } => {
        format!("still running after {:.2}s, stopped", elapsed.as_secs_f64())
      }
    }
  }

  fn wait(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
  ) -> anyhow::Result<Option<std::process::ExitStatus>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
      if let Some(status) = child.try_wait()? {
        return Ok(Some(status));
      }
      let now = std::time::Instant::now();
      if now >= deadline {
        return Ok(None);
      }
      std::thread::sleep(Self::POLL.min(deadline - now));
    }
  }

  fn stop(child: &mut std::process::Child) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
      let group = format!("-{}", child.id());
      let signal = |name: &str| {
        ProcessBuilder::new("kill")
          .args([name, "--", &group])
          .output_unchecked()
      };
      signal("-TERM")?;
      if Self::wait(child, Self::GRACE)?.is_some() {
        signal("-KILL")?;
        return Ok(());
      }
      signal("-KILL")?;
    }
    child.kill().ok();
    child.wait()?;
    Ok(())
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
      std::time::Duration::from_millis(timeout_ms),
    )
    .unwrap();
    (run, std::fs::read_to_string(log).unwrap())
  }

  #[test]
  fn clean_exits_pass_and_output_is_logged() {
    let (run, log) = run("echo out; echo err >&2", 5000);
    assert!(run.passed(true), "{run:?}");
    assert_eq!(log, "out\nerr\n");
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
