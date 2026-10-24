use wkit_core::{Shell, shell::style};

use crate::{Member, Phase, Report, Result, TaskOutcome, TaskStatus};

#[derive(Debug, Clone, Copy)]
pub struct Runner {
  jobs: usize,
}

impl Runner {
  pub const DEFAULT_JOBS: usize = 8;

  pub fn new(jobs: usize) -> Self {
    Self { jobs: jobs.max(1) }
  }

  pub fn run<F>(
    &self,
    shell: &mut Shell,
    title: &str,
    done: &str,
    members: &[Member],
    task: F,
  ) -> Result<Report>
  where
    F: Fn(&Member, &Phase) -> TaskOutcome + Sync,
  {
    let bar = shell.progress(title, members.len() as u64);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut outcomes: Vec<Option<TaskOutcome>> = vec![None; members.len()];
    let printed = std::thread::scope(|scope| -> Result<()> {
      for _ in 0..self.jobs.min(members.len()) {
        let sender = sender.clone();
        let (bar, next, task) = (&bar, &next, &task);
        scope.spawn(move || {
          loop {
            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(member) = members.get(index) else {
              break;
            };
            let outcome = task(member, &Phase::new(bar.clone(), &member.path));
            if sender.send((index, outcome)).is_err() {
              break;
            }
          }
        });
      }
      drop(sender);
      for (index, outcome) in receiver {
        bar.inc(1);
        Self::print(shell, done, &members[index].path, &outcome)?;
        outcomes[index] = Some(outcome);
      }
      Ok(())
    });
    bar.finish_and_clear();
    printed?;
    Ok(Report {
      outcomes: members
        .iter()
        .zip(outcomes)
        .map(|(m, o)| {
          (
            m.path.clone(),
            o.unwrap_or_else(|| TaskOutcome::failed("did not run")),
          )
        })
        .collect(),
    })
  }

  fn print(shell: &mut Shell, done: &str, path: &str, outcome: &TaskOutcome) -> Result<()> {
    let line = format!("{path}  {}", outcome.detail);
    match outcome.status {
      TaskStatus::Done => shell.status_with_style(done, line, &style::HEADER)?,
      TaskStatus::Fresh => shell.verbose(|s| s.status_with_style("Fresh", &line, &style::DIM))?,
      TaskStatus::Warning => shell.status_with_style("Warning", line, &style::WARN)?,
      TaskStatus::Failed => shell.status_with_style("Failed", line, &style::ERROR)?,
    }
    Ok(())
  }
}
