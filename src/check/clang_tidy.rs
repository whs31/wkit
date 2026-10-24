use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  path::{Path, PathBuf},
};

use anyhow::Context as _;
use rayon::prelude::*;

use super::{CheckOptions, Outcome};
use crate::{
  Result, Shell,
  manifest::ClangTidyCheck,
  shell::{plural, style},
  tool::Tool,
};

const DATABASE: &str = "compile_commands.json";

#[derive(Debug, Clone)]
pub struct ClangTidy {
  tool: Tool,
  config: ClangTidyCheck,
}

impl ClangTidy {
  pub const NAME: &str = "clang-tidy";
  const CACHE_DIR: &str = ".clang-tidy-cache";
  const CONFIG: &str = ".clang-tidy";

  pub fn is_configured(root: &Path) -> bool {
    discover(root, None).is_ok()
  }

  pub fn new(tool: Tool, config: ClangTidyCheck) -> Self {
    Self { tool, config }
  }

  fn header_filter(&self, root: &Path) -> String {
    let sep = r"[/\\]";
    let parts: Vec<_> = root
      .components()
      .filter_map(|component| match component {
        std::path::Component::Prefix(p) => Some(regex::escape(&p.as_os_str().to_string_lossy())),
        std::path::Component::Normal(n) => Some(regex::escape(&n.to_string_lossy())),
        _ => None,
      })
      .collect();
    let lead = if cfg!(windows) { "(?i)^" } else { "^/" };
    let dirs: Vec<_> = self.config.dirs.iter().map(|d| regex::escape(d)).collect();
    format!("{lead}{}{sep}(?:{}){sep}", parts.join(sep), dirs.join("|"))
  }

  fn flags(&self, root: &Path) -> Vec<String> {
    let mut flags = vec![
      "--quiet".to_owned(),
      format!("--header-filter={}", self.header_filter(root)),
    ];
    if self.config.warnings_as_errors {
      flags.push("--warnings-as-errors=*".to_owned());
    }
    flags
  }

  fn analyze(
    &self,
    root: &Path,
    db_dir: &Path,
    flags: &[String],
    source: &Path,
    (directory, command): &(PathBuf, String),
    cache: Option<&Cache>,
  ) -> Result<Run> {
    let includes = db_dir.join(format!(
      "{}.includes",
      digest(source.as_os_str().as_encoded_bytes())
    ));
    let mut process = self
      .tool
      .command()
      .cwd(root)
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
    let run = Run {
      success: output.status.success(),
      output: format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
      ),
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
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| directory.join(l)),
        )
        .collect();
      cache.store(source, &run, command, &inputs);
    }
    Ok(run)
  }

  fn render(
    display: impl Fn(&Path) -> String,
    diagnostics: &BTreeSet<Diagnostic>,
    verbose: bool,
  ) -> Vec<String> {
    let (error, warn, note, dim) = (style::ERROR, style::WARN, style::NOTE, style::DIM);
    let mut lines = Vec::new();
    let mut counts = BTreeMap::<&str, usize>::new();
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
        display(&d.path),
        d.line,
        d.column
      ));
      if verbose {
        lines.extend(d.details.iter().map(|l| format!("  {dim}{l}{dim:#}")));
      }
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
    if let Some((_, most)) = counts.first() {
      lines.push(format!("{dim}by check:{dim:#}"));
      let width = most.to_string().len();
      lines.extend(
        counts
          .iter()
          .map(|(check, n)| format!("  {n:>width$}  {check}")),
      );
    }
    lines
  }

  pub fn run(&self, root: &Path, shell: &mut Shell, options: &CheckOptions) -> Result<Outcome> {
    let started = std::time::SystemTime::now();
    let db_path = discover(
      root,
      options
        .build_dir
        .as_deref()
        .or(self.config.build_dir.as_deref()),
    )?;
    let mut db = Database::load(&db_path)?;
    db.strip(&self.config.strip_flags);
    let scratch = tempfile::Builder::new()
      .prefix("wkit-clang-tidy-")
      .tempdir()?;
    db.write_to(scratch.path())?;

    let real_root = real(root);
    let display = |path: &Path| {
      real(path)
        .strip_prefix(&real_root)
        .or_else(|_| path.strip_prefix(root))
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
    };
    let wanted: Vec<_> = self.config.dirs.iter().map(|d| real_root.join(d)).collect();
    let sources: Vec<_> = db
      .sources()
      .into_iter()
      .filter(|(source, _)| wanted.iter().any(|dir| real(source).starts_with(dir)))
      .collect();
    if sources.is_empty() {
      return Ok(Outcome::skipped(format!(
        "no translation units under {}",
        self.config.dirs.join(", ")
      )));
    }

    let flags = self.flags(&real_root);
    let cache = if self.config.cache {
      let configs = configs(sources.iter().map(|(s, _)| s));
      let setup = serde_json::json!([self.tool.to_string(), flags, configs]).to_string();
      Some(Cache::new(
        db_path.parent().unwrap_or(root).join(Self::CACHE_DIR),
        setup,
        started,
      )?)
    } else {
      None
    };

    self.tool.command().arg("--version").output()?;
    shell.status(
      "Checking",
      format_args!(
        "{} with {} ({})",
        plural(sources.len(), "source"),
        self.tool,
        display(&db_path)
      ),
    )?;
    let progress = shell.progress("Checking", sources.len() as u64);
    let pool = rayon::ThreadPoolBuilder::new()
      .num_threads(options.jobs.clamp(1, sources.len()))
      .build()?;
    let runs = pool.install(|| {
      sources
        .par_iter()
        .map(|(source, entry)| {
          let cached = cache
            .as_ref()
            .filter(|_| options.cache)
            .and_then(|c| c.lookup(source, &entry.1));
          let run = match cached {
            Some(run) => run,
            None => self.analyze(root, scratch.path(), &flags, source, entry, cache.as_ref())?,
          };
          progress.inc(1);
          Ok((source, &entry.0, run))
        })
        .collect::<Result<Vec<_>>>()
    });
    progress.finish_and_clear();
    let runs = runs?;
    if let Some(cache) = &cache {
      cache.prune(sources.iter().map(|(s, _)| s));
    }

    Ok(self.summarize(&runs, sources.len(), display, options.verbose))
  }

  fn summarize(
    &self,
    runs: &[(&PathBuf, &PathBuf, Run)],
    sources: usize,
    display: impl Fn(&Path) -> String + Copy,
    verbose: bool,
  ) -> Outcome {
    let cached = runs.iter().filter(|(_, _, r)| r.cached).count();
    let mut diagnostics = BTreeSet::new();
    let mut failures = Vec::new();
    for (source, directory, run) in runs {
      let parsed = Diagnostic::parse_all(&run.output, directory);
      if !run.success && parsed.is_empty() {
        failures.push((source, run));
      }
      diagnostics.extend(parsed);
    }
    let mut details = Self::render(display, &diagnostics, verbose);
    for (source, run) in &failures {
      let error = style::ERROR;
      details.push(format!(
        "{error}error{error:#}: clang-tidy failed on {} without a diagnostic",
        display(source)
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
    let files: BTreeSet<_> = diagnostics.iter().map(|d| &d.path).collect();
    let mut summary = if diagnostics.is_empty() {
      format!("{} clean", plural(sources, "source"))
    } else {
      format!(
        "{} in {}",
        plural(diagnostics.len(), "finding"),
        plural(files.len(), "file")
      )
    };
    if !failures.is_empty() {
      summary = format!("{summary}, {} failed", failures.len());
    }
    if cached > 0 {
      summary = format!("{summary} ({cached} cached)");
    }
    Outcome {
      details,
      ..if failed {
        Outcome::failed(summary, Vec::new())
      } else {
        Outcome::passed(summary)
      }
    }
  }
}

fn real(path: &Path) -> PathBuf {
  if cfg!(windows) {
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
  } else {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
  }
}

fn configs<'a>(
  sources: impl IntoIterator<Item = &'a PathBuf>,
) -> BTreeMap<PathBuf, Option<String>> {
  let dirs: BTreeSet<_> = sources
    .into_iter()
    .flat_map(|s| s.ancestors().skip(1))
    .collect();
  dirs
    .into_iter()
    .map(|dir| dir.join(ClangTidy::CONFIG))
    .filter(|config| config.is_file())
    .map(|config| {
      let digest = digest_file(&config);
      (config, digest)
    })
    .collect()
}

fn digest(bytes: &[u8]) -> String {
  let hash = |seed: u8| {
    let mut hasher = std::hash::DefaultHasher::new();
    std::hash::Hasher::write_u8(&mut hasher, seed);
    std::hash::Hasher::write(&mut hasher, bytes);
    std::hash::Hasher::finish(&hasher)
  };
  format!("{:016x}{:016x}", hash(0), hash(1))
}

fn digest_file(path: &Path) -> Option<String> {
  std::fs::read(path).ok().map(|bytes| digest(&bytes))
}

fn discover(root: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
  if let Some(dir) = explicit {
    let path = root.join(dir).join(DATABASE);
    anyhow::ensure!(path.is_file(), "`{}` does not exist", path.display());
    return Ok(path);
  }
  let mut candidates = vec![root.join(DATABASE), root.join("build").join(DATABASE)];
  for parent in [root.join("build"), root.to_owned()] {
    for entry in std::fs::read_dir(&parent).into_iter().flatten().flatten() {
      if parent != root
        || entry
          .file_name()
          .to_string_lossy()
          .starts_with("cmake-build-")
      {
        candidates.push(entry.path().join(DATABASE));
      }
    }
  }
  candidates
    .into_iter()
    .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
    .max()
    .map(|(_, path)| path)
    .context("no compile_commands.json found in build/*/; pass BUILD_DIR")
}

#[derive(Debug, Clone)]
struct Database {
  entries: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl Database {
  fn load(path: &Path) -> Result<Self> {
    let entries = serde_json::from_str(&fs_err::read_to_string(path)?)
      .with_context(|| format!("failed to parse `{}`", path.display()))?;
    Ok(Self { entries })
  }

  fn strip(&mut self, flags: &[String]) {
    for entry in &mut self.entries {
      if let Some(serde_json::Value::String(command)) = entry.get_mut("command") {
        for flag in flags {
          *command = command.replace(&format!(" {flag}"), "");
        }
      }
      if let Some(serde_json::Value::Array(arguments)) = entry.get_mut("arguments") {
        arguments.retain(|a| !a.as_str().is_some_and(|a| flags.iter().any(|f| f == a)));
      }
    }
  }

  fn write_to(&self, dir: &Path) -> Result<()> {
    Ok(fs_err::write(
      dir.join(DATABASE),
      serde_json::to_string(&self.entries)?,
    )?)
  }

  fn sources(&self) -> BTreeMap<PathBuf, (PathBuf, String)> {
    let mut sources = BTreeMap::new();
    for entry in &self.entries {
      let field = |key| entry.get(key).and_then(serde_json::Value::as_str);
      let (Some(directory), Some(file)) = (field("directory"), field("file")) else {
        continue;
      };
      let directory = PathBuf::from(directory);
      let command = serde_json::to_string(entry).expect("entries serialize");
      sources
        .entry(directory.join(file))
        .or_insert((directory, command));
    }
    sources
  }
}

#[derive(Debug, Clone)]
struct Diagnostic {
  path: PathBuf,
  line: u32,
  column: u32,
  severity: String,
  message: String,
  check: String,
  details: Vec<String>,
}

impl Diagnostic {
  fn parse_all(output: &str, base: &Path) -> Vec<Self> {
    let re = lazy_regex::regex!(
      r"^(?<path>.+?):(?<line>\d+):(?<column>\d+): (?<severity>fatal error|error|warning|note): (?<message>.*?)(?: \[(?<check>[^\]]+)\])?$"
    );
    let mut found: Vec<Self> = Vec::new();
    for line in output.lines() {
      let Some(c) = re.captures(line).filter(|c| &c["severity"] != "note") else {
        if let Some(open) = found.last_mut()
          && !line.trim().is_empty()
          && !line.ends_with(" generated.")
        {
          open.details.push(line.to_owned());
        }
        continue;
      };
      let check = c.name("check").map_or("", |m| m.as_str());
      let (check, severity) = match check.strip_suffix(",-warnings-as-errors") {
        Some(original) => (original, "warning"),
        None => (check, &c["severity"]),
      };
      found.push(Self {
        path: base.join(&c["path"]),
        line: c["line"].parse().unwrap_or_default(),
        column: c["column"].parse().unwrap_or_default(),
        severity: severity.to_owned(),
        message: c["message"].to_owned(),
        check: check.to_owned(),
        details: Vec::new(),
      });
    }
    found
  }

  fn is_error(&self) -> bool {
    self.severity != "warning"
  }

  fn key(&self) -> (&Path, u32, u32, &str, &str, &str) {
    (
      &self.path,
      self.line,
      self.column,
      &self.check,
      &self.severity,
      &self.message,
    )
  }
}

impl PartialEq for Diagnostic {
  fn eq(&self, other: &Self) -> bool {
    self.key() == other.key()
  }
}

impl Eq for Diagnostic {}

impl PartialOrd for Diagnostic {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Diagnostic {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.key().cmp(&other.key())
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
  success: bool,
  output: String,
  cached: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
  key: String,
  inputs: BTreeMap<PathBuf, String>,
  success: bool,
  output: String,
}

#[derive(Debug)]
struct Cache {
  dir: PathBuf,
  setup: String,
  started: std::time::SystemTime,
  digests: std::sync::Mutex<HashMap<PathBuf, Option<String>>>,
}

impl Cache {
  const VERSION: u32 = 1;

  fn new(dir: PathBuf, setup: String, started: std::time::SystemTime) -> Result<Self> {
    fs_err::create_dir_all(&dir)?;
    Ok(Self {
      dir,
      setup,
      started,
      digests: std::sync::Mutex::default(),
    })
  }

  fn entry_path(&self, source: &Path) -> PathBuf {
    self.dir.join(format!(
      "{}.json",
      digest(source.as_os_str().as_encoded_bytes())
    ))
  }

  fn lookup(&self, source: &Path, command: &str) -> Option<Run> {
    let entry: CacheEntry =
      serde_json::from_str(&std::fs::read_to_string(self.entry_path(source)).ok()?).ok()?;
    let fresh = entry.key == self.key(command)
      && entry
        .inputs
        .iter()
        .all(|(path, digest)| self.digest(path).as_ref() == Some(digest));
    fresh.then_some(Run {
      success: entry.success,
      output: entry.output,
      cached: true,
    })
  }

  fn store(&self, source: &Path, run: &Run, command: &str, inputs: &[PathBuf]) {
    let unchanged_during_run = |path: &Path| {
      std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|modified| modified < self.started)
    };
    let Some(inputs) = inputs
      .iter()
      .map(|path| {
        self
          .digest(path)
          .filter(|_| unchanged_during_run(path))
          .map(|d| (path.clone(), d))
      })
      .collect::<Option<_>>()
    else {
      return;
    };
    let entry = CacheEntry {
      key: self.key(command),
      inputs,
      success: run.success,
      output: run.output.clone(),
    };
    let path = self.entry_path(source);
    let temporary = path.with_extension("tmp");
    if std::fs::write(
      &temporary,
      serde_json::to_string(&entry).expect("entries serialize"),
    )
    .is_ok()
    {
      let _ = std::fs::rename(&temporary, &path);
    }
  }

  fn prune<'a>(&self, sources: impl IntoIterator<Item = &'a PathBuf>) {
    let keep: std::collections::HashSet<_> = sources
      .into_iter()
      .filter_map(|s| self.entry_path(s).file_name().map(ToOwned::to_owned))
      .collect();
    for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
      if !keep.contains(&entry.file_name()) {
        let _ = std::fs::remove_file(entry.path());
      }
    }
  }

  fn key(&self, command: &str) -> String {
    digest(format!("{}\n{}\n{command}", Self::VERSION, self.setup).as_bytes())
  }

  fn digest(&self, path: &Path) -> Option<String> {
    let mut digests = self.digests.lock().expect("not poisoned");
    digests
      .entry(path.to_owned())
      .or_insert_with(|| digest_file(path))
      .clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn header_filter_matches_project_dirs_only() {
    let tidy = ClangTidy::new(Tool::system("clang-tidy"), ClangTidyCheck::default());
    let abs = |path: &str| format!("{}{path}", if cfg!(windows) { "C:" } else { "" });
    let filter =
      regex::Regex::new(&tidy.header_filter(Path::new(&abs("/home/me/my.repo")))).unwrap();
    assert!(filter.is_match(&abs("/home/me/my.repo/include/a.h")));
    assert!(filter.is_match(&abs("/home/me/my.repo/src/x/b.hpp")));
    assert!(!filter.is_match(&abs("/home/me/my.repo/build/gen/a.h")));
    assert!(!filter.is_match(&abs("/home/me/myXrepo/src/a.h")));
    assert!(!filter.is_match(&abs("/usr/include/stdio.h")));
  }

  #[test]
  fn digests_are_stable_and_distinct() {
    assert_eq!(digest(b"a"), digest(b"a"));
    assert_ne!(digest(b"a"), digest(b"b"));
    assert_eq!(digest(b"a").len(), 32);
  }

  #[test]
  fn strips_flags_and_lists_first_commands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(DATABASE);
    std::fs::write(
      &path,
      r#"[
        {"directory": "/b", "file": "/src/a.cpp", "command": "c++ -mno-direct-extern-access -c /src/a.cpp"},
        {"directory": "/b", "file": "/src/a.cpp", "command": "c++ -DSECOND -c /src/a.cpp"},
        {"directory": "/b", "file": "rel.cpp", "arguments": ["c++", "-mno-direct-extern-access", "-c", "rel.cpp"]}
      ]"#,
    )
    .unwrap();
    let mut db = Database::load(&path).unwrap();
    db.strip(&["-mno-direct-extern-access".to_owned()]);
    let sources = db.sources();
    assert_eq!(sources.len(), 2);
    let (dir_a, a) = &sources[Path::new("/src/a.cpp")];
    assert_eq!(dir_a, Path::new("/b"));
    assert!(a.contains("c++ -c /src/a.cpp") && !a.contains("SECOND"));
    assert!(!sources[Path::new("/b/rel.cpp")].1.contains("extern-access"));
  }

  #[test]
  fn discovers_the_newest_database() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert!(discover(root, None).is_err());
    for sub in ["build/Debug", "build/Release"] {
      std::fs::create_dir_all(root.join(sub)).unwrap();
      std::fs::write(root.join(sub).join(DATABASE), "[]").unwrap();
      std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
      discover(root, None).unwrap(),
      root.join("build/Release").join(DATABASE)
    );
    assert_eq!(
      discover(root, Some(Path::new("build/Debug"))).unwrap(),
      root.join("build/Debug").join(DATABASE)
    );
    assert!(discover(root, Some(Path::new("nope"))).is_err());
  }

  #[test]
  fn parses_findings_with_their_details() {
    let output = "2 warnings generated.\nsrc/main.cpp:3:43: error: use nullptr [modernize-use-nullptr,-warnings-as-errors]\n    3 | int* p = 0;\n      |          ^\n/abs/a.h:1:1: warning: bad header [readability-x]\n/abs/a.h:1:1: note: declared here\nerror: unknown argument: '-x' [clang-diagnostic-error]\n";
    let found = Diagnostic::parse_all(output, Path::new("/root"));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].path, Path::new("/root/src/main.cpp"));
    assert_eq!((found[0].line, found[0].column), (3, 43));
    assert_eq!(found[0].severity, "warning");
    assert_eq!(found[0].check, "modernize-use-nullptr");
    assert_eq!(found[0].details.len(), 2);
    assert_eq!(found[1].path, Path::new("/abs/a.h"));
    assert_eq!(found[1].details.len(), 2);
    assert_eq!(
      Diagnostic::parse_all("error: unknown argument", Path::new("/")),
      Vec::<Diagnostic>::new()
    );
  }

  fn run() -> Run {
    Run {
      success: true,
      output: "ok".into(),
      cached: false,
    }
  }

  #[test]
  fn cache_replays_until_an_input_changes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.cpp");
    let header = dir.path().join("a.h");
    std::fs::write(&source, "int a;").unwrap();
    std::fs::write(&header, "int b;").unwrap();
    let started = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let cache = Cache::new(dir.path().join("cache"), "setup".into(), started).unwrap();
    assert!(cache.lookup(&source, "cmd").is_none());
    cache.store(&source, &run(), "cmd", &[source.clone(), header.clone()]);
    let hit = cache.lookup(&source, "cmd").unwrap();
    assert!(hit.cached && hit.success && hit.output == "ok");
    assert!(cache.lookup(&source, "other command").is_none());

    std::fs::write(&header, "int c;").unwrap();
    let fresh = Cache::new(dir.path().join("cache"), "setup".into(), started).unwrap();
    assert!(fresh.lookup(&source, "cmd").is_none());
    let other_setup = Cache::new(dir.path().join("cache"), "v2".into(), started).unwrap();
    assert!(other_setup.lookup(&source, "cmd").is_none());
  }

  #[test]
  fn files_edited_during_the_run_are_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.cpp");
    std::fs::write(&source, "int a;").unwrap();
    let started = std::time::SystemTime::now() - std::time::Duration::from_secs(5);
    let cache = Cache::new(dir.path().join("cache"), "s".into(), started).unwrap();
    cache.store(&source, &run(), "cmd", std::slice::from_ref(&source));
    assert!(cache.lookup(&source, "cmd").is_none());
  }

  #[test]
  fn prune_drops_entries_of_removed_sources() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.cpp"), dir.path().join("b.cpp"));
    for path in [&a, &b] {
      std::fs::write(path, "").unwrap();
    }
    let started = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let cache = Cache::new(dir.path().join("cache"), "s".into(), started).unwrap();
    cache.store(&a, &run(), "cmd", std::slice::from_ref(&a));
    cache.store(&b, &run(), "cmd", std::slice::from_ref(&b));
    cache.prune([&a]);
    assert!(cache.entry_path(&a).is_file() && !cache.entry_path(&b).exists());
  }
}
