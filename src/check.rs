mod clang_tidy;
mod mms;

use std::path::{Path, PathBuf};

pub use self::clang_tidy::ClangTidy;
use self::mms::Mms;
use crate::{
  ProcessBuilder, Result, Shell,
  format::{Formatter, Mode},
  fs::SourceSet,
  manifest::{ClippyCheck, CustomCheck, Project, QmllintCheck, Tools, VersionSection},
  shell::{elapsed, plural, style},
  tool::Tool,
  version::VersionSet,
};

#[derive(Debug, Clone, PartialEq, Eq, smart_default::SmartDefault)]
pub struct CheckOptions {
  #[default(std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get))]
  pub jobs: usize,
  #[default(true)]
  pub cache: bool,
  pub build_dir: Option<PathBuf>,
  pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum Status {
  #[display("ok")]
  Passed,
  #[display("FAILED")]
  Failed,
  #[display("skipped")]
  Skipped,
}

impl Status {
  pub fn style(self) -> &'static anstyle::Style {
    match self {
      Self::Passed => &style::HEADER,
      Self::Failed => &style::ERROR,
      Self::Skipped => &style::DIM,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
  pub status: Status,
  pub summary: String,
  pub details: Vec<String>,
}

impl Outcome {
  pub fn passed(summary: impl Into<String>) -> Self {
    Self {
      status: Status::Passed,
      summary: summary.into(),
      details: Vec::new(),
    }
  }

  pub fn failed(summary: impl Into<String>, details: Vec<String>) -> Self {
    Self {
      status: Status::Failed,
      summary: summary.into(),
      details,
    }
  }

  pub fn skipped(summary: impl Into<String>) -> Self {
    Self {
      status: Status::Skipped,
      summary: summary.into(),
      details: Vec::new(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageReport {
  pub name: String,
  pub outcome: Outcome,
  pub elapsed: std::time::Duration,
}

impl StageReport {
  pub fn failed(&self) -> bool {
    self.outcome.status == Status::Failed
  }
}

#[derive(Debug)]
enum Stage {
  Format(Formatter),
  Versions(VersionSection),
  Clippy(ClippyCheck),
  ClangTidy(ClangTidy),
  Secrets,
  Whitespace,
  PreCommit(Tools),
  Qmllint(Tool, QmllintCheck),
  Mms(Mms),
  Custom(CustomCheck),
}

#[derive(Debug)]
pub struct Checker {
  root: PathBuf,
  stages: Vec<(String, Stage)>,
}

impl Checker {
  pub fn plan(project: &Project, only: &[String], skip: &[String]) -> Result<Self> {
    let m = &project.manifest;
    let formatter = Formatter::new(project)?;
    let mut planned = vec![
      (
        "format".to_owned(),
        (m.check.format && !formatter.is_empty()).then_some(Stage::Format(formatter)),
      ),
      (
        "versions".into(),
        m.version
          .clone()
          .filter(|_| m.check.versions)
          .map(Stage::Versions),
      ),
      (
        "clippy".into(),
        m.clippy_enabled()
          .then(|| Stage::Clippy(m.check.clippy.clone())),
      ),
      (
        ClangTidy::NAME.into(),
        m.clang_tidy_enabled().then(|| {
          Stage::ClangTidy(ClangTidy::new(
            project.tool(ClangTidy::NAME),
            m.check.clang_tidy.clone(),
          ))
        }),
      ),
      ("secrets".into(), m.check.secrets.then_some(Stage::Secrets)),
      (
        "whitespace".into(),
        m.check.whitespace.then_some(Stage::Whitespace),
      ),
      (
        "pre-commit".into(),
        m.check
          .pre_commit
          .then(|| Stage::PreCommit(m.tools.clone())),
      ),
      (
        "qmllint".into(),
        m.check
          .qmllint
          .enabled
          .then(|| Stage::Qmllint(project.tool("qmllint"), m.check.qmllint.clone())),
      ),
      (
        "mms".into(),
        Mms::plan(&project.root, &m.check.mms, m.version.as_ref())?.map(Stage::Mms),
      ),
    ];
    planned.extend(
      m.check
        .custom
        .iter()
        .map(|c| (c.name.clone(), Some(Stage::Custom(c.clone())))),
    );

    let known: Vec<_> = planned.iter().map(|(name, _)| name.as_str()).collect();
    if let Some(name) = only
      .iter()
      .chain(skip)
      .find(|n| !known.contains(&n.as_str()))
    {
      anyhow::bail!(
        "unknown check `{name}`; expected one of: {}",
        known.join(", ")
      );
    }
    let mut stages = Vec::new();
    for (name, stage) in planned {
      if skip.contains(&name) || (!only.is_empty() && !only.contains(&name)) {
        continue;
      }
      match stage {
        Some(stage) => stages.push((name, stage)),
        None if only.contains(&name) => {
          anyhow::bail!("check `{name}` is disabled for this project")
        }
        None => {}
      }
    }
    Ok(Self {
      root: project.root.clone(),
      stages,
    })
  }

  pub fn names(&self) -> Vec<String> {
    self.stages.iter().map(|(name, _)| name.clone()).collect()
  }

  pub fn run(&self, shell: &mut Shell, options: &CheckOptions) -> Result<Vec<StageReport>> {
    let mut reports = Vec::with_capacity(self.stages.len());
    for (name, stage) in &self.stages {
      let start = std::time::Instant::now();
      let outcome = self.run_stage(stage, shell, options).unwrap_or_else(|e| {
        let details = e.chain().skip(1).flat_map(|c| {
          c.to_string()
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
        });
        Outcome::failed(e.to_string(), details.collect())
      });
      for line in &outcome.details {
        writeln!(shell.err(), "{line}")?;
      }
      reports.push(StageReport {
        name: name.clone(),
        outcome,
        elapsed: start.elapsed(),
      });
    }
    let first_line = |r: &StageReport| {
      r.outcome
        .summary
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
    };
    let name_width = reports
      .iter()
      .map(|r| r.name.len())
      .max()
      .unwrap_or_default();
    let summary_width = reports
      .iter()
      .map(|r| first_line(r).len())
      .max()
      .unwrap_or_default();
    for report in &reports {
      let status = report.outcome.status;
      shell.status_with_style(
        status,
        format_args!(
          "{:<name_width$}  {:<summary_width$}  {}",
          report.name,
          first_line(report),
          elapsed(report.elapsed)
        ),
        status.style(),
      )?;
    }
    Ok(reports)
  }

  fn run_stage(&self, stage: &Stage, shell: &mut Shell, options: &CheckOptions) -> Result<Outcome> {
    let root = &self.root;
    match stage {
      Stage::Format(formatter) => {
        let files = formatter.run(shell, Mode::Check, options.jobs)?;
        Ok(if files.is_empty() {
          Outcome::passed("formatted")
        } else {
          Outcome::failed(
            format!(
              "{} unformatted; run `wkit fmt`",
              plural(files.len(), "file")
            ),
            files.iter().map(|f| format!("  {}", f.display())).collect(),
          )
        })
      }
      Stage::Versions(section) => versions(root, section, shell),
      Stage::Clippy(config) => {
        let command = clippy(root, config);
        shell.status("Checking", "clippy")?;
        shell.verbose(|s| s.status("Running", &command))?;
        let status = command.status_unchecked()?;
        Ok(if status.success() {
          Outcome::passed("no warnings")
        } else {
          Outcome::failed(format!("cargo clippy failed ({status})"), Vec::new())
        })
      }
      Stage::ClangTidy(tidy) => tidy.run(root, shell, options),
      Stage::Secrets => secrets::run(root, shell),
      Stage::Whitespace => whitespace(root, shell),
      Stage::PreCommit(tools) => pre_commit::run(root, tools, shell),
      Stage::Qmllint(tool, config) => qmllint(root, tool, config, shell),
      Stage::Mms(mms) => mms.run(root, shell),
      Stage::Custom(check) => {
        let task = check.task();
        shell.status(
          "Checking",
          check.description.as_deref().unwrap_or(&check.name),
        )?;
        shell.verbose(|s| s.status("Running", format_args!("`{}`", task.run.line(&[]))))?;
        let status = task.to_process(root, &[]).status_unchecked()?;
        Ok(if status.success() {
          Outcome::passed("passed")
        } else {
          Outcome::failed(format!("failed ({status})"), Vec::new())
        })
      }
    }
  }
}

fn versions(root: &Path, section: &VersionSection, shell: &mut Shell) -> Result<Outcome> {
  shell.status(
    "Checking",
    format_args!("versions ({})", plural(section.files.len(), "file")),
  )?;
  let set = VersionSet::load(root, section)?;
  let current = set.current()?;
  let mismatches = set.mismatches(&current);
  if mismatches.is_empty() {
    return Ok(Outcome::passed(format!("all at {current}")));
  }
  Ok(Outcome::failed(
    format!(
      "{} disagree with {current}",
      plural(mismatches.len(), "file")
    ),
    mismatches.iter().map(|m| format!("  {m}")).collect(),
  ))
}

fn clippy(root: &Path, config: &ClippyCheck) -> ProcessBuilder {
  let mut cargo = ProcessBuilder::new("cargo").cwd(root);
  if let Some(toolchain) = &config.toolchain {
    cargo = cargo.arg(format!("+{toolchain}"));
  }
  cargo = cargo.args(["clippy", "--workspace"]);
  if config.all_targets {
    cargo = cargo.arg("--all-targets");
  }
  if config.all_features {
    cargo = cargo.arg("--all-features");
  } else if !config.features.is_empty() {
    cargo = cargo.arg("--features").arg(config.features.join(","));
  }
  cargo = cargo.args(&config.args);
  if config.deny_warnings {
    if !config.args.iter().any(|a| a == "--") {
      cargo = cargo.arg("--");
    }
    cargo = cargo.args(["-D", "warnings"]);
  }
  cargo
}

fn whitespace(root: &Path, shell: &mut Shell) -> Result<Outcome> {
  const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
  let git = || ProcessBuilder::new("git").cwd(root);
  if !git()
    .args(["rev-parse", "--is-inside-work-tree"])
    .output_unchecked()
    .is_ok_and(|o| o.status.success())
  {
    return Ok(Outcome::skipped("not a git work tree"));
  }
  shell.status("Checking", "whitespace in uncommitted changes")?;
  let has_head = git()
    .args(["rev-parse", "--quiet", "--verify", "HEAD"])
    .output_unchecked()?
    .status
    .success();
  let diff = git().args([
    "diff",
    "--check",
    if has_head { "HEAD" } else { EMPTY_TREE },
    "--",
    ".",
  ]);
  let output = diff.output_unchecked()?;
  if output.status.success() {
    return Ok(Outcome::passed("clean"));
  }
  let problems = whitespace_problems(&String::from_utf8_lossy(&output.stdout));
  if problems.is_empty() {
    diff.check(output.status, &output.stderr)?;
  }
  Ok(Outcome::failed(plural(problems.len(), "problem"), problems))
}

fn whitespace_problems(output: &str) -> Vec<String> {
  output
    .lines()
    .filter(|line| !line.starts_with(['+', '-', ' ']))
    .filter_map(|line| lazy_regex::regex_captures!(r"^(.+?):(\d+): (.+?)\.?$", line))
    .map(|(_, path, line, problem)| format!("  {path}:{line}: {problem}"))
    .collect()
}

fn qmllint_sources(root: &Path, config: &QmllintCheck) -> Result<Vec<PathBuf>> {
  let exclude = config.exclude.clone().unwrap_or_else(|| {
    SourceSet::DEFAULT_EXCLUDES
      .iter()
      .map(|&e| e.to_owned())
      .collect()
  });
  let sources = SourceSet::new(root)
    .extensions(["qml"])
    .include(&config.include)
    .exclude(exclude)
    .collect()?;
  Ok(
    sources
      .into_iter()
      .map(|p| p.strip_prefix(root).map(Path::to_owned).unwrap_or(p))
      .collect(),
  )
}

fn qmllint(root: &Path, tool: &Tool, config: &QmllintCheck, shell: &mut Shell) -> Result<Outcome> {
  let sources = qmllint_sources(root, config)?;
  if sources.is_empty() {
    return Ok(Outcome::skipped("no QML files"));
  }
  let command = tool.command().cwd(root).args(&config.args).args(&sources);
  shell.status(
    "Checking",
    format_args!("{} with {tool}", plural(sources.len(), "QML file")),
  )?;
  shell.verbose(|s| s.status("Running", &command))?;
  let output = command.output_unchecked()?;
  let text = format!(
    "{}{}",
    String::from_utf8_lossy(&output.stdout),
    String::from_utf8_lossy(&output.stderr)
  );
  let warnings = text
    .lines()
    .filter(|l| l.starts_with("Warning:") || l.starts_with("Error:"))
    .count();
  if output.status.success() && warnings == 0 {
    return Ok(Outcome::passed(format!(
      "{} clean",
      plural(sources.len(), "file")
    )));
  }
  Ok(Outcome::failed(
    if warnings == 0 {
      format!("qmllint failed ({})", output.status)
    } else {
      plural(warnings, "warning")
    },
    text.lines().map(|l| format!("  {l}")).collect(),
  ))
}

mod secrets {
  use super::{Outcome, Path, Result, Shell, SourceSet, plural};

  pub const ALLOW_MARKER: &str = "wkit: allow-secret";
  const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;
  const KINDS: &[(&str, &str)] = &[
    (
      "GitHub token",
      r"\b(?:github_pat_[A-Za-z0-9_]{22,}|gh[pousr]_[A-Za-z0-9]{36,})",
    ),
    ("GitLab token", r"\bglpat-[A-Za-z0-9_-]{20,}"),
    ("AWS access key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    ("private key", r"-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----"),
    ("Slack token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
    ("Anthropic API key", r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
    ("OpenAI API key", r"\bsk-proj-[A-Za-z0-9_-]{20,}"),
    ("Google API key", r"\bAIza[0-9A-Za-z_-]{35}\b"),
  ];

  pub fn scan(text: &str) -> Vec<(usize, &'static str)> {
    static SET: std::sync::LazyLock<regex::RegexSet> = std::sync::LazyLock::new(|| {
      regex::RegexSet::new(KINDS.iter().map(|(_, pattern)| pattern)).expect("valid")
    });
    text
      .lines()
      .enumerate()
      .filter(|(_, line)| !line.contains(ALLOW_MARKER))
      .flat_map(|(index, line)| {
        SET
          .matches(line)
          .into_iter()
          .map(move |kind| (index + 1, KINDS[kind].0))
      })
      .collect()
  }

  fn readable(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE_SIZE {
      return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
      return None;
    }
    String::from_utf8(bytes).ok()
  }

  pub fn run(root: &Path, shell: &mut Shell) -> Result<Outcome> {
    let files = SourceSet::new(root).hidden(true).collect()?;
    shell.status(
      "Checking",
      format_args!("{} for secrets", plural(files.len(), "file")),
    )?;
    let mut details = Vec::new();
    for file in &files {
      let Some(text) = readable(file) else {
        continue;
      };
      let shown = file.strip_prefix(root).unwrap_or(file).display();
      details.extend(
        scan(&text)
          .into_iter()
          .map(|(line, kind)| format!("  {shown}:{line}: {kind}")),
      );
    }
    if details.is_empty() {
      return Ok(Outcome::passed(format!(
        "{} clean",
        plural(files.len(), "file")
      )));
    }
    let found = plural(details.len(), "secret");
    details.push(format!(
      "  mark intentional test data with `{ALLOW_MARKER}` on the same line"
    ));
    Ok(Outcome::failed(format!("{found} found"), details))
  }
}

mod pre_commit {
  use super::{Outcome, Path, Result, Shell, Tools, plural};

  const CONFIGS: &[&str] = &[".pre-commit-config.yaml", ".pre-commit-config.yml"];

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Repo {
    pub rev: Option<String>,
    pub line: usize,
    pub hooks: Vec<String>,
  }

  pub fn repos(text: &str) -> Vec<Repo> {
    let mut repos: Vec<Repo> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
      let line = raw.split(" #").next().unwrap_or_default().trim();
      let Some((key, value)) = line.trim_start_matches("- ").trim().split_once(':') else {
        continue;
      };
      let value = value.trim().trim_matches(['"', '\'']).to_owned();
      match (key.trim(), repos.last_mut()) {
        ("repo", _) => repos.push(Repo {
          rev: None,
          line: index + 1,
          hooks: Vec::new(),
        }),
        ("rev", Some(repo)) => {
          repo.rev = Some(value);
          repo.line = index + 1;
        }
        ("id", Some(repo)) => repo.hooks.push(value),
        _ => {}
      }
    }
    repos
  }

  pub fn mismatches(root: &Path, tools: &Tools, config: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for repo in repos(text) {
      let Some(rev) = &repo.rev else {
        continue;
      };
      for hook in &repo.hooks {
        let pinned = tools.get(hook).is_some() || Tools::DEFAULTS.iter().any(|(t, _)| t == hook);
        let tool = tools.resolve(hook, root);
        let Some(version) = tool.version().filter(|_| pinned) else {
          continue;
        };
        if rev.trim_start_matches('v') != version {
          found.push(format!(
            "  {config}:{}: {hook} is pinned to {rev}, but `[tools]` uses {version}",
            repo.line
          ));
        }
      }
    }
    found
  }

  pub fn run(root: &Path, tools: &Tools, shell: &mut Shell) -> Result<Outcome> {
    let Some((name, path)) = CONFIGS
      .iter()
      .map(|name| (*name, root.join(name)))
      .find(|(_, path)| path.is_file())
    else {
      return Ok(Outcome::skipped("no .pre-commit-config.yaml"));
    };
    shell.status("Checking", format_args!("tool pins in {name}"))?;
    let mismatches = mismatches(root, tools, name, &fs_err::read_to_string(&path)?);
    Ok(if mismatches.is_empty() {
      Outcome::passed("pins match `[tools]`")
    } else {
      Outcome::failed(plural(mismatches.len(), "stale pin"), mismatches)
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::manifest::Manifest;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn plans_stages_from_the_manifest() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"rust\"]\n\n[[check.custom]]\nname = \"meta\"\nrun = \"true\"\n",
    );
    let checker = Checker::plan(&project, &[], &[]).unwrap();
    assert_eq!(
      checker.names(),
      [
        "format",
        "clippy",
        "secrets",
        "whitespace",
        "pre-commit",
        "meta"
      ]
    );
    let only = Checker::plan(&project, &["clippy".into(), "meta".into()], &[]).unwrap();
    assert_eq!(only.names(), ["clippy", "meta"]);
    let skip = Checker::plan(&project, &[], &["format".into(), "whitespace".into()]).unwrap();
    assert_eq!(skip.names(), ["clippy", "secrets", "pre-commit", "meta"]);
  }

  #[test]
  fn rejects_unknown_and_disabled_stages() {
    let (_dir, project) = project("[project]\nlanguages = [\"rust\"]\n");
    let unknown = Checker::plan(&project, &["clipy".into()], &[]).unwrap_err();
    assert!(unknown.to_string().contains("unknown check `clipy`"));
    let disabled = Checker::plan(&project, &["clang-tidy".into()], &[]).unwrap_err();
    assert!(disabled.to_string().contains("disabled"));
  }

  #[test]
  fn failures_and_errors_are_reported_per_stage() {
    let (dir, project) = project(
      "[project]\nlanguages = [\"c\"]\n\n[version]\nfiles = [\"CMakeLists.txt\"]\n\n[[check.custom]]\nname = \"fails\"\nrun = [\"false\"]\n",
    );
    std::fs::write(
      dir.path().join("CMakeLists.txt"),
      "project(x VERSION 1.0.0)\n",
    )
    .unwrap();
    let checker = Checker::plan(
      &project,
      &["versions".into(), "clang-tidy".into(), "fails".into()],
      &[],
    )
    .unwrap();
    let reports = checker
      .run(&mut Shell::sink(), &CheckOptions::default())
      .unwrap();
    let failed: Vec<_> = reports
      .iter()
      .map(|r| (r.name.as_str(), r.failed()))
      .collect();
    assert_eq!(
      failed,
      [("versions", false), ("clang-tidy", true), ("fails", true)]
    );
    assert!(reports[1].outcome.summary.contains("compile_commands.json"));
  }

  #[test]
  fn clippy_command_follows_the_config() {
    let root = Path::new("/repo");
    assert_eq!(
      clippy(root, &ClippyCheck::default()).to_string(),
      "cargo clippy --workspace --all-targets -- -D warnings"
    );
    let config = ClippyCheck {
      toolchain: Some("nightly".into()),
      all_targets: false,
      deny_warnings: false,
      features: vec!["a".into(), "b".into()],
      args: vec!["--locked".into()],
      ..ClippyCheck::default()
    };
    assert_eq!(
      clippy(root, &config).to_string(),
      "cargo +nightly clippy --workspace --features a,b --locked"
    );
  }

  #[test]
  fn parses_git_diff_check() {
    let output = "src/a.cpp:3: trailing whitespace.\n+int x; \nREADME.md:10: space before tab in indent.\n+ \tfoo\n";
    assert_eq!(
      whitespace_problems(output),
      [
        "  src/a.cpp:3: trailing whitespace",
        "  README.md:10: space before tab in indent"
      ]
    );
  }

  #[test]
  fn lints_qml_sources_outside_build_outputs() {
    let dir = tempfile::tempdir().unwrap();
    for file in ["qml/Main.qml", "build/Debug/gen.qml", "qml/readme.md"] {
      let path = dir.path().join(file);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, "").unwrap();
    }
    assert_eq!(
      qmllint_sources(dir.path(), &QmllintCheck::default()).unwrap(),
      [PathBuf::from("qml/Main.qml")]
    );
  }

  #[test]
  fn finds_tokens_and_keys() {
    let github = format!("token = \"ghp_{}\"", "a".repeat(36));
    let gitlab = format!("glpat-{}", "x".repeat(20));
    let key = format!("-----BEGIN {}PRIVATE KEY-----", "RSA ");
    let text = format!("ok\n{github}\n{gitlab} {}\n{key}\n", secrets::ALLOW_MARKER);
    assert_eq!(
      secrets::scan(&text),
      [(2, "GitHub token"), (4, "private key")]
    );
    assert_eq!(secrets::scan("ghp_short glpat- AKIA"), []);
  }

  #[test]
  fn reports_secrets_relative_to_the_root() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
      dir.path().join(".env"),
      format!("AWS={}\n", "AKIA".to_owned() + &"Q".repeat(16)),
    )
    .unwrap();
    std::fs::write(dir.path().join("blob.bin"), b"\0AKIAQQQQQQQQQQQQQQQQ").unwrap();
    let outcome = secrets::run(dir.path(), &mut Shell::sink()).unwrap();
    assert_eq!(outcome.status, Status::Failed);
    assert_eq!(outcome.details[0], "  .env:1: AWS access key");
  }

  #[test]
  fn reads_pre_commit_repos_revs_and_hooks() {
    let text = "repos:\n  - repo: local\n    hooks:\n      - id: just-fmt\n        entry: just fmt\n  - repo: https://github.com/pre-commit/mirrors-clang-format\n    rev: v23.1.3  # keep in sync\n    hooks:\n      - id: clang-format\n        args: [--style=file]\n";
    let repos = pre_commit::repos(text);
    assert_eq!(repos.len(), 2);
    assert_eq!(
      (repos[0].hooks.as_slice(), repos[0].rev.as_deref()),
      (["just-fmt".to_owned()].as_slice(), None)
    );
    assert_eq!(repos[1].rev.as_deref(), Some("v23.1.3"));
    assert_eq!(repos[1].hooks, ["clang-format"]);
    assert_eq!(repos[1].line, 7);
  }

  #[test]
  fn compares_pre_commit_revs_with_pinned_tools() {
    let tools = Manifest::parse("[project]\nlanguages = [\"cpp\"]\n[tools]\nblack = \"25.9.0\"\n")
      .unwrap()
      .tools;
    let text = "repos:\n  - repo: https://github.com/pre-commit/mirrors-clang-format\n    rev: v19.1.6\n    hooks:\n      - id: clang-format\n  - repo: https://github.com/psf/black\n    rev: 25.9.0\n    hooks:\n      - id: black\n  - repo: https://github.com/pre-commit/pre-commit-hooks\n    rev: v5.0.0\n    hooks:\n      - id: trailing-whitespace\n";
    assert_eq!(
      pre_commit::mismatches(Path::new("/repo"), &tools, ".pre-commit-config.yaml", text),
      ["  .pre-commit-config.yaml:3: clang-format is pinned to v19.1.6, but `[tools]` uses 23.1.3"]
    );
  }
}
