use wkit_core::{Error, ProcessBuilder, Result, Shell, fs::SourceSet, tool::Tool};
use wkit_manifest::Project;

use crate::Mode;

#[derive(Debug, Clone)]
pub struct ClangFormat {
  root: std::path::PathBuf,
  tool: Tool,
  sources: SourceSet,
}

impl ClangFormat {
  pub const TOOL: &str = "clang-format";
  pub const BATCH: usize = 64;

  pub fn new(project: &Project) -> Option<Self> {
    let manifest = &project.manifest;
    if !manifest.cpp_format_enabled() {
      return None;
    }
    let config = &manifest.format.cpp;
    Some(Self {
      root: project.root.clone(),
      tool: project.tool(Self::TOOL),
      sources: SourceSet::new(&project.root)
        .extensions(&config.extensions)
        .include(&config.include)
        .exclude(&config.exclude),
    })
  }

  pub fn tool(&self) -> &Tool {
    &self.tool
  }

  pub fn command(&self, mode: Mode, batch: &[std::path::PathBuf]) -> ProcessBuilder {
    let flags: &[&str] = match mode {
      Mode::Write => &["-i"],
      Mode::Check => &["--dry-run", "--Werror"],
    };
    self
      .tool
      .command()
      .cwd(&self.root)
      .args(flags)
      .arg("--")
      .args(batch)
  }

  pub fn run(&self, shell: &mut Shell, mode: Mode, jobs: usize) -> Result<Vec<std::path::PathBuf>> {
    let sources: Vec<_> = self
      .sources
      .collect()?
      .into_iter()
      .map(|p| {
        p.strip_prefix(&self.root)
          .map(std::path::Path::to_owned)
          .unwrap_or(p)
      })
      .collect();
    if sources.is_empty() {
      shell.verbose(|s| s.note("no C/C++ sources to format"))?;
      return Ok(Vec::new());
    }
    self.tool.command().arg("--version").output()?;
    shell.status(
      mode.label(),
      format_args!("{} C/C++ files with {}", sources.len(), self.tool),
    )?;
    let progress = shell.progress(mode.label(), sources.len() as u64);
    let before = match mode {
      Mode::Write => Some(self.digests(&sources)?),
      Mode::Check => None,
    };
    let batches: Vec<_> = sources.chunks(Self::BATCH).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let flagged = std::sync::Mutex::new(Vec::new());
    let failure = std::sync::Mutex::new(None);
    std::thread::scope(|scope| {
      for _ in 0..jobs.clamp(1, batches.len()) {
        scope.spawn(|| {
          while failure.lock().expect("not poisoned").is_none() {
            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(batch) = batches.get(index) else {
              break;
            };
            match self.run_batch(mode, batch) {
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
    progress.finish_and_clear();
    if let Some(e) = failure.into_inner().expect("not poisoned") {
      return Err(e);
    }
    let mut files = match before {
      Some(before) => {
        let after = self.digests(&sources)?;
        sources
          .into_iter()
          .zip(before.into_iter().zip(after))
          .filter(|(_, (b, a))| b != a)
          .map(|(path, _)| path)
          .collect()
      }
      None => flagged.into_inner().expect("not poisoned"),
    };
    files.sort();
    files.dedup();
    Ok(files)
  }

  fn run_batch(&self, mode: Mode, batch: &[std::path::PathBuf]) -> Result<Vec<std::path::PathBuf>> {
    let command = self.command(mode, batch);
    let output = command.output_unchecked()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let flagged = match mode {
      Mode::Write => Vec::new(),
      Mode::Check => Self::violations(&stderr),
    };
    if output.status.success() || !flagged.is_empty() {
      Ok(flagged)
    } else {
      Err(Error::ProcessFailed {
        command: command.to_string(),
        status: output.status,
        stderr: stderr.into_owned(),
      })
    }
  }

  fn violations(stderr: &str) -> Vec<std::path::PathBuf> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r"(?m)^(?<path>.+?):\d+:\d+: (?:error|warning): code should be clang-formatted",
      )
      .expect("valid")
    });
    RE.captures_iter(stderr)
      .map(|c| std::path::PathBuf::from(&c["path"]))
      .collect()
  }

  fn digests(&self, sources: &[std::path::PathBuf]) -> Result<Vec<u64>> {
    sources
      .iter()
      .map(|path| {
        let path = self.root.join(path);
        let bytes = std::fs::read(&path).map_err(|source| Error::Read { path, source })?;
        Ok(std::hash::BuildHasher::hash_one(
          &std::hash::BuildHasherDefault::<std::hash::DefaultHasher>::default(),
          bytes,
        ))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn violations_name_each_flagged_file() {
    let stderr = "src/a.cpp:1:4: error: code should be clang-formatted [-Wclang-format-violations]\nint  main(){}\n   ^\nsrc/dir with space/b.h:10:1: warning: code should be clang-formatted [-Wclang-format-violations]\n";
    assert_eq!(
      ClangFormat::violations(stderr),
      [
        std::path::PathBuf::from("src/a.cpp"),
        std::path::PathBuf::from("src/dir with space/b.h"),
      ]
    );
    assert!(ClangFormat::violations("error: unknown argument\n").is_empty());
  }
}
