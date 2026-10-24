mod cache_entry;
mod compile_database;
mod diagnostic;
mod tidy_cache;
mod tidy_run;

use wkit_core::{Shell, shell::style, tool::Tool};
use wkit_manifest::check::ClangTidyCheck;

use self::{
  compile_database::CompileDatabase, diagnostic::Diagnostic, tidy_cache::TidyCache,
  tidy_run::TidyRun,
};
use crate::{Check, CheckOptions, Digest, Outcome, Result};

#[derive(Debug, Clone)]
pub struct ClangTidy {
  root: std::path::PathBuf,
  tool: Tool,
  config: ClangTidyCheck,
}

impl ClangTidy {
  pub const NAME: &str = "clang-tidy";
  pub const TOOL: &str = "clang-tidy";
  pub const CACHE_DIR: &str = ".clang-tidy-cache";
  pub const CONFIG: &str = ".clang-tidy";

  pub fn is_configured(root: &std::path::Path) -> bool {
    CompileDatabase::discover(root, None).is_ok()
  }

  pub fn new(root: impl Into<std::path::PathBuf>, tool: Tool, config: ClangTidyCheck) -> Self {
    Self {
      root: root.into(),
      tool,
      config,
    }
  }

  pub fn header_filter(&self, root: &std::path::Path) -> String {
    let sep = r"[/\\]";
    let mut parts = Vec::new();
    for component in root.components() {
      match component {
        std::path::Component::Prefix(p) => {
          parts.push(regex::escape(&p.as_os_str().to_string_lossy()));
        }
        std::path::Component::Normal(n) => parts.push(regex::escape(&n.to_string_lossy())),
        _ => {}
      }
    }
    let lead = if cfg!(windows) { "(?i)^" } else { "^/" };
    let dirs: Vec<_> = self.config.dirs.iter().map(|d| regex::escape(d)).collect();
    format!("{lead}{}{sep}(?:{}){sep}", parts.join(sep), dirs.join("|"))
  }

  fn flags(&self, root: &std::path::Path) -> Vec<String> {
    let mut flags = vec![
      "--quiet".to_owned(),
      format!("--header-filter={}", self.header_filter(root)),
    ];
    if self.config.warnings_as_errors {
      flags.push("--warnings-as-errors=*".to_owned());
    }
    flags
  }

  fn real(path: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
      std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
    } else {
      std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
    }
  }

  fn configs<'a>(
    sources: impl IntoIterator<Item = &'a std::path::PathBuf>,
  ) -> std::collections::BTreeMap<std::path::PathBuf, Option<String>> {
    let mut dirs = std::collections::BTreeSet::new();
    for source in sources {
      dirs.extend(source.ancestors().skip(1).map(std::path::Path::to_owned));
    }
    dirs
      .into_iter()
      .map(|dir| dir.join(Self::CONFIG))
      .filter(|config| config.is_file())
      .map(|config| {
        let digest = Digest::of_file(&config).map(|d| d.to_string());
        (config, digest)
      })
      .collect()
  }

  fn analyze(
    &self,
    db_dir: &std::path::Path,
    flags: &[String],
    source: &std::path::Path,
    directory: &std::path::Path,
    command: &str,
    cache: Option<&TidyCache>,
  ) -> Result<TidyRun> {
    let includes = db_dir.join(format!(
      "{}.includes",
      Digest::of(source.as_os_str().as_encoded_bytes())
    ));
    let mut process = self
      .tool
      .command()
      .cwd(&self.root)
      .arg("-p")
      .arg(db_dir)
      .args(flags);
    for flag in [
      "-sys-header-deps".as_ref(),
      "-header-include-file".as_ref(),
      includes.as_os_str(),
    ] {
      let mut extra = std::ffi::OsString::from("--extra-arg=");
      extra.push(flag);
      process = process.arg("--extra-arg=-Xclang").arg(extra);
    }
    let output = process.arg(source).output_unchecked()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let run = TidyRun {
      source: source.to_owned(),
      success: output.status.success(),
      output: text,
      cached: false,
    };
    if let Some(cache) = cache
      && let Ok(listed) = std::fs::read_to_string(&includes)
      && (run.success || !Diagnostic::parse_all(&run.output, directory).is_empty())
    {
      let inputs: Vec<_> = std::iter::once(source.to_owned())
        .chain(
          listed
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| directory.join(l.trim())),
        )
        .collect();
      cache.store(&run, command, &inputs);
    }
    Ok(run)
  }

  fn display(&self, real_root: &std::path::Path, path: &std::path::Path) -> String {
    let real = Self::real(path);
    real
      .strip_prefix(real_root)
      .or_else(|_| path.strip_prefix(&self.root))
      .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
  }

  fn render(
    &self,
    real_root: &std::path::Path,
    diagnostics: &std::collections::BTreeSet<Diagnostic>,
    verbose: bool,
  ) -> Vec<String> {
    let (error, warn, note, dim) = (style::ERROR, style::WARN, style::NOTE, style::DIM);
    let mut lines = Vec::new();
    for d in diagnostics {
      let severity = if d.is_error() { error } else { warn };
      let check = if d.check.is_empty() {
        String::new()
      } else {
        format!(" {dim}[{}]{dim:#}", d.check)
      };
      lines.push(format!(
        "{severity}{}{severity:#}: {}{check}",
        d.severity, d.message
      ));
      lines.push(format!(
        "  {note}-->{note:#} {}:{}:{}",
        self.display(real_root, &d.path),
        d.line,
        d.column
      ));
      if verbose {
        lines.extend(d.details.iter().map(|l| format!("  {dim}{l}{dim:#}")));
      }
    }
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for d in diagnostics {
      *counts
        .entry(if d.check.is_empty() {
          "(compiler)"
        } else {
          &d.check
        })
        .or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    if !counts.is_empty() {
      lines.push(format!("{dim}by check:{dim:#}"));
      let width = counts[0].1.to_string().len();
      lines.extend(
        counts
          .iter()
          .map(|(check, n)| format!("  {n:>width$}  {check}")),
      );
    }
    lines
  }
}

impl Check for ClangTidy {
  fn name(&self) -> String {
    Self::NAME.to_owned()
  }

  #[allow(clippy::too_many_lines)]
  fn run(&self, shell: &mut Shell, options: &CheckOptions) -> Result<Outcome> {
    let started = std::time::SystemTime::now();
    let explicit = options
      .build_dir
      .as_deref()
      .or(self.config.build_dir.as_deref());
    let db_path = CompileDatabase::discover(&self.root, explicit)?;
    let mut db = CompileDatabase::load(&db_path)?;
    db.strip(&self.config.strip_flags);
    let scratch = tempfile::Builder::new()
      .prefix("wkit-clang-tidy-")
      .tempdir()
      .map_err(|source| wkit_core::Error::Write {
        path: std::env::temp_dir(),
        source,
      })?;
    db.write_to(scratch.path())?;

    let real_root = Self::real(&self.root);
    let wanted: Vec<_> = self.config.dirs.iter().map(|d| real_root.join(d)).collect();
    let sources: Vec<_> = db
      .sources()
      .into_iter()
      .filter(|(source, _)| {
        let real = Self::real(source);
        wanted.iter().any(|dir| real.starts_with(dir))
      })
      .collect();
    if sources.is_empty() {
      return Ok(Outcome::skipped(format!(
        "no translation units under {}",
        self.config.dirs.join(", ")
      )));
    }

    let flags = self.flags(&real_root);
    let cache = if self.config.cache {
      let configs = Self::configs(sources.iter().map(|(s, _)| s));
      let setup = serde_json::json!([self.tool.to_string(), flags, configs]).to_string();
      let dir = db_path.parent().unwrap_or(&self.root).join(Self::CACHE_DIR);
      Some(TidyCache::new(dir, setup, started)?)
    } else {
      None
    };

    self.tool.command().arg("--version").output()?;
    shell.status(
      "Checking",
      format_args!(
        "{} with {} ({})",
        Outcome::plural(sources.len(), "source"),
        self.tool,
        self.display(&real_root, &db_path)
      ),
    )?;
    let progress = shell.progress("Checking", sources.len() as u64);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let runs = std::sync::Mutex::new(Vec::with_capacity(sources.len()));
    let failure = std::sync::Mutex::new(None);
    std::thread::scope(|scope| {
      for _ in 0..options.jobs.clamp(1, sources.len()) {
        scope.spawn(|| {
          while failure.lock().expect("not poisoned").is_none() {
            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some((source, (directory, command))) = sources.get(index) else {
              break;
            };
            let cached = cache
              .as_ref()
              .filter(|_| options.cache)
              .and_then(|c| c.lookup(source, command));
            let run = match cached {
              Some(run) => Ok(run),
              None => self.analyze(
                scratch.path(),
                &flags,
                source,
                directory,
                command,
                cache.as_ref(),
              ),
            };
            match run {
              Ok(run) => runs
                .lock()
                .expect("not poisoned")
                .push((directory.clone(), run)),
              Err(e) => {
                failure.lock().expect("not poisoned").get_or_insert(e);
              }
            }
            progress.inc(1);
          }
        });
      }
    });
    progress.finish_and_clear();
    if let Some(e) = failure.into_inner().expect("not poisoned") {
      return Err(e);
    }
    if let Some(cache) = &cache {
      cache.prune(sources.iter().map(|(s, _)| s));
    }

    let runs = runs.into_inner().expect("not poisoned");
    let cached = runs.iter().filter(|(_, r)| r.cached).count();
    let mut diagnostics = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for (directory, run) in &runs {
      let parsed = Diagnostic::parse_all(&run.output, directory);
      if !run.success && parsed.is_empty() {
        failures.push(run);
      }
      diagnostics.extend(parsed);
    }
    let mut details = self.render(&real_root, &diagnostics, options.verbose);
    failures.sort_by(|a, b| a.source.cmp(&b.source));
    for run in &failures {
      details.push(format!(
        "{}error{:#}: clang-tidy failed on {} without a diagnostic",
        style::ERROR,
        style::ERROR,
        self.display(&real_root, &run.source)
      ));
      let lines: Vec<_> = run.output.lines().collect();
      details.extend(
        lines[lines.len().saturating_sub(20)..]
          .iter()
          .map(|l| format!("  {}{l}{:#}", style::DIM, style::DIM)),
      );
    }

    let failed = !failures.is_empty()
      || diagnostics.iter().any(Diagnostic::is_error)
      || (self.config.warnings_as_errors && !diagnostics.is_empty());
    let files: std::collections::BTreeSet<_> = diagnostics.iter().map(|d| &d.path).collect();
    let mut summary = if diagnostics.is_empty() {
      format!("{} clean", Outcome::plural(sources.len(), "source"))
    } else {
      format!(
        "{} in {}",
        Outcome::plural(diagnostics.len(), "finding"),
        Outcome::plural(files.len(), "file")
      )
    };
    if !failures.is_empty() {
      summary = format!("{summary}, {} failed", failures.len());
    }
    if cached > 0 {
      summary = format!("{summary} ({cached} cached)");
    }
    Ok(if failed {
      Outcome::failed(summary, details)
    } else {
      Outcome {
        details,
        ..Outcome::passed(summary)
      }
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn header_filter_matches_project_dirs_only() {
    let tidy = ClangTidy::new(
      "/repo",
      Tool::system("clang-tidy"),
      ClangTidyCheck::default(),
    );
    let filter =
      regex::Regex::new(&tidy.header_filter(std::path::Path::new("/home/me/my.repo"))).unwrap();
    assert!(filter.is_match("/home/me/my.repo/include/a.h"));
    assert!(filter.is_match("/home/me/my.repo/src/x/b.hpp"));
    assert!(!filter.is_match("/home/me/my.repo/build/gen/a.h"));
    assert!(!filter.is_match("/home/me/myXrepo/src/a.h"));
    assert!(!filter.is_match("/usr/include/stdio.h"));
  }
}
