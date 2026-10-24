use wkit_core::Result;

#[derive(Debug, Clone, Copy)]
pub struct Batches {
  size: usize,
  jobs: usize,
}

impl Batches {
  pub fn new(size: usize, jobs: usize) -> Self {
    Self {
      size: size.max(1),
      jobs: jobs.max(1),
    }
  }

  pub fn run<F>(
    &self,
    sources: &[std::path::PathBuf],
    progress: &indicatif::ProgressBar,
    work: F,
  ) -> Result<Vec<std::path::PathBuf>>
  where
    F: Fn(&[std::path::PathBuf]) -> Result<Vec<std::path::PathBuf>> + Sync,
  {
    let batches: Vec<_> = sources.chunks(self.size).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let flagged = std::sync::Mutex::new(Vec::new());
    let failure = std::sync::Mutex::new(None);
    std::thread::scope(|scope| {
      for _ in 0..self.jobs.min(batches.len()) {
        scope.spawn(|| {
          while failure.lock().expect("not poisoned").is_none() {
            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(batch) = batches.get(index) else {
              break;
            };
            match work(batch) {
              Ok(found) => flagged.lock().expect("not poisoned").extend(found),
              Err(e) => {
                failure.lock().expect("not poisoned").get_or_insert(e);
              }
            }
            progress.inc(batch.len() as u64);
          }
        });
      }
    });
    if let Some(e) = failure.into_inner().expect("not poisoned") {
      return Err(e);
    }
    let mut flagged = flagged.into_inner().expect("not poisoned");
    flagged.sort();
    flagged.dedup();
    Ok(flagged)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_source_is_visited_once_and_flags_are_sorted() {
    let sources: Vec<std::path::PathBuf> = (0..10).rev().map(|i| format!("{i}.c").into()).collect();
    let progress = indicatif::ProgressBar::hidden();
    let flagged = Batches::new(3, 4)
      .run(&sources, &progress, |batch| {
        Ok(
          batch
            .iter()
            .filter(|p| *p < std::path::Path::new("5"))
            .cloned()
            .collect(),
        )
      })
      .unwrap();
    assert_eq!(progress.position(), 10);
    let names: Vec<_> = flagged.iter().map(|p| p.display().to_string()).collect();
    assert_eq!(names, ["0.c", "1.c", "2.c", "3.c", "4.c"]);
  }

  #[test]
  fn the_first_failure_is_returned() {
    let sources = vec![std::path::PathBuf::from("a")];
    let result = Batches::new(1, 2).run(&sources, &indicatif::ProgressBar::hidden(), |_| {
      Err(wkit_core::Error::ProgramNotFound {
        program: "boom".into(),
      })
    });
    assert!(result.is_err());
  }
}
