use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::{
  ProcessBuilder, Result, Shell,
  detect::RustfmtChannel,
  fs::SourceSet,
  manifest::{ExtraFormat, Manifest, Project},
  tool::Tool,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
  Write,
  Check,
}

impl Mode {
  pub fn label(self) -> &'static str {
    match self {
      Self::Write => "Formatting",
      Self::Check => "Checking",
    }
  }
}

#[derive(Debug, Clone)]
pub struct Formatter {
  pub rustfmt: Option<Rustfmt>,
  pub tools: Vec<ToolFormatter>,
}

impl Formatter {
  pub fn new(project: &Project) -> Result<Self> {
    Ok(Self {
      rustfmt: Rustfmt::new(project)?,
      tools: Kind::ALL
        .into_iter()
        .filter_map(|kind| ToolFormatter::new(project, kind))
        .collect(),
    })
  }

  pub fn is_empty(&self) -> bool {
    self.rustfmt.is_none() && self.tools.is_empty()
  }

  pub fn names(&self) -> Vec<String> {
    let rustfmt = self.rustfmt.as_ref().map(|_| "rustfmt".to_owned());
    rustfmt
      .into_iter()
      .chain(self.tools.iter().map(|f| f.tool.to_string()))
      .collect()
  }

  pub fn run(&self, shell: &mut Shell, mode: Mode, jobs: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if let Some(rustfmt) = &self.rustfmt {
      files.extend(rustfmt.run(shell, mode)?);
    }
    for formatter in &self.tools {
      files.extend(formatter.run(shell, mode, jobs)?);
    }
    Ok(files)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  ClangFormat,
  Qmlformat,
  Black,
  Stylua,
}

impl Kind {
  pub const ALL: [Self; 4] = [
    Self::ClangFormat,
    Self::Qmlformat,
    Self::Black,
    Self::Stylua,
  ];

  pub fn tool(self) -> &'static str {
    match self {
      Self::ClangFormat => "clang-format",
      Self::Qmlformat => "qmlformat",
      Self::Black => "black",
      Self::Stylua => "stylua",
    }
  }

  pub fn language(self) -> &'static str {
    match self {
      Self::ClangFormat => "C/C++",
      Self::Qmlformat => "QML",
      Self::Black => "Python",
      Self::Stylua => "Lua",
    }
  }

  fn config(self, manifest: &Manifest) -> Option<(&ExtraFormat, &'static [&'static str])> {
    match self {
      Self::ClangFormat => None,
      Self::Qmlformat => Some((&manifest.format.qmlformat, &["qml"])),
      Self::Black => Some((&manifest.format.black, &["py", "pyi"])),
      Self::Stylua => Some((&manifest.format.stylua, &["lua"])),
    }
  }

  fn is_configured(self, root: &Path) -> bool {
    match self {
      Self::ClangFormat => true,
      Self::Qmlformat => root.join(".qmlformat.ini").is_file(),
      Self::Black => std::fs::read_to_string(root.join("pyproject.toml"))
        .is_ok_and(|text| text.lines().any(|l| l.trim() == "[tool.black]")),
      Self::Stylua => ["stylua.toml", ".stylua.toml"]
        .iter()
        .any(|name| root.join(name).is_file()),
    }
  }

  fn flags(self, mode: Mode) -> &'static [&'static str] {
    match (self, mode) {
      (Self::ClangFormat | Self::Qmlformat, Mode::Write) => &["-i"],
      (Self::ClangFormat, Mode::Check) => &["--dry-run", "--Werror"],
      (Self::Black, Mode::Write) => &["-q"],
      (Self::Black | Self::Stylua, Mode::Check) => &["--check"],
      (Self::Stylua, Mode::Write) | (Self::Qmlformat, Mode::Check) => &[],
    }
  }

  fn violations(self, root: &Path, output: &str) -> Vec<PathBuf> {
    let pattern = match self {
      Self::ClangFormat => {
        lazy_regex::regex!(
          r"(?m)^(?<path>.+?):\d+:\d+: (?:error|warning): code should be clang-formatted"
        )
      }
      Self::Black => lazy_regex::regex!(r"(?m)^would reformat (?<path>.+?)\r?$"),
      Self::Stylua => lazy_regex::regex!(r"(?m)^Diff in (?<path>.+?):\r?$"),
      Self::Qmlformat => return Vec::new(),
    };
    pattern
      .captures_iter(output)
      .map(|c| {
        let path = Path::new(&c["path"]);
        path.strip_prefix(root).unwrap_or(path).to_owned()
      })
      .collect()
  }
}

#[derive(Debug, Clone)]
pub struct ToolFormatter {
  pub kind: Kind,
  pub tool: Tool,
  root: PathBuf,
  sources: SourceSet,
  args: Vec<String>,
}

impl ToolFormatter {
  const BATCH: usize = 64;

  pub fn new(project: &Project, kind: Kind) -> Option<Self> {
    let manifest = &project.manifest;
    let owned = |items: &[&str]| items.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
    let (extensions, include, exclude, args) = match kind.config(manifest) {
      None if manifest.cpp_format_enabled() => {
        let cpp = &manifest.format.cpp;
        (
          cpp.extensions.clone(),
          &cpp.include,
          cpp.exclude.clone(),
          Vec::new(),
        )
      }
      Some((config, extensions))
        if config
          .enabled
          .unwrap_or_else(|| kind.is_configured(&project.root)) =>
      {
        (
          config
            .extensions
            .clone()
            .unwrap_or_else(|| owned(extensions)),
          &config.include,
          config
            .exclude
            .clone()
            .unwrap_or_else(|| owned(SourceSet::DEFAULT_EXCLUDES)),
          config.args.clone(),
        )
      }
      _ => return None,
    };
    Some(Self {
      kind,
      tool: project.tool(kind.tool()),
      sources: SourceSet::new(&project.root)
        .extensions(extensions)
        .include(include)
        .exclude(exclude),
      root: project.root.clone(),
      args,
    })
  }

  fn command(&self, mode: Mode, batch: &[PathBuf]) -> ProcessBuilder {
    let command = self
      .tool
      .command()
      .cwd(&self.root)
      .args(&self.args)
      .args(self.kind.flags(mode));
    match self.kind {
      Kind::Qmlformat => command.args(batch),
      Kind::ClangFormat | Kind::Black | Kind::Stylua => command.arg("--").args(batch),
    }
  }

  pub fn run(&self, shell: &mut Shell, mode: Mode, jobs: usize) -> Result<Vec<PathBuf>> {
    let sources: Vec<_> = self
      .sources
      .collect()?
      .into_iter()
      .map(|p| p.strip_prefix(&self.root).map(Path::to_owned).unwrap_or(p))
      .collect();
    let language = self.kind.language();
    if sources.is_empty() {
      shell.verbose(|s| s.note(format_args!("no {language} sources to format")))?;
      return Ok(Vec::new());
    }
    self.tool.command().arg("--version").output()?;
    let count = sources.len();
    shell.status(
      mode.label(),
      format_args!(
        "{count} {language} file{} with {}",
        if count == 1 { "" } else { "s" },
        self.tool
      ),
    )?;
    let progress = shell.progress(mode.label(), sources.len() as u64);
    let before = match mode {
      Mode::Write => Some(fingerprints(&self.root, &sources)?),
      Mode::Check => None,
    };
    let flagged = rayon::ThreadPoolBuilder::new()
      .num_threads(jobs)
      .build()?
      .install(|| {
        sources
          .par_chunks(Self::BATCH)
          .map(|batch| {
            let found = self.run_batch(mode, batch);
            progress.inc(batch.len() as u64);
            found
          })
          .collect::<Result<Vec<_>>>()
      });
    progress.finish_and_clear();
    let mut flagged: Vec<_> = flagged?.into_iter().flatten().collect();
    if let Some(before) = before {
      let after = fingerprints(&self.root, &sources)?;
      flagged = sources
        .iter()
        .zip(before.iter().zip(after))
        .filter(|(_, (b, a))| *b != a)
        .map(|(p, _)| p.clone())
        .collect();
    }
    flagged.sort();
    flagged.dedup();
    Ok(flagged)
  }

  fn run_batch(&self, mode: Mode, batch: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if mode == Mode::Check && self.kind == Kind::Qmlformat {
      return batch
        .iter()
        .filter_map(|path| self.differs(path).transpose())
        .collect();
    }
    let command = self.command(mode, batch);
    let output = command.output_unchecked()?;
    let flagged = match mode {
      Mode::Write => Vec::new(),
      Mode::Check => self.kind.violations(
        &self.root,
        &format!(
          "{}\n{}",
          String::from_utf8_lossy(&output.stdout),
          String::from_utf8_lossy(&output.stderr)
        ),
      ),
    };
    if flagged.is_empty() {
      command.check(output.status, &output.stderr)?;
    }
    Ok(flagged)
  }

  fn differs(&self, path: &Path) -> Result<Option<PathBuf>> {
    let formatted = self.command(Mode::Check, &[path.to_owned()]).output()?;
    let current = fs_err::read(self.root.join(path))?;
    Ok((formatted.stdout != current).then(|| path.to_owned()))
  }
}

fn fingerprints(root: &Path, sources: &[PathBuf]) -> Result<Vec<u64>> {
  sources
    .iter()
    .map(|path| {
      let bytes = fs_err::read(root.join(path))?;
      Ok(std::hash::BuildHasher::hash_one(
        &std::hash::BuildHasherDefault::<std::hash::DefaultHasher>::default(),
        bytes,
      ))
    })
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rustfmt {
  root: PathBuf,
  pub channel: RustfmtChannel,
  args: Vec<String>,
}

impl Rustfmt {
  pub fn new(project: &Project) -> Result<Option<Self>> {
    let manifest = &project.manifest;
    if !manifest.rust_format_enabled() {
      return Ok(None);
    }
    let config = &manifest.format.rust;
    let channel = match config.channel {
      Some(channel) => channel,
      None => RustfmtChannel::from_path(&project.root)?,
    };
    Ok(Some(Self {
      root: project.root.clone(),
      channel,
      args: config.args.clone(),
    }))
  }

  fn command(&self, mode: Mode) -> ProcessBuilder {
    let mut cargo = ProcessBuilder::new("cargo").cwd(&self.root);
    if self.channel == RustfmtChannel::Nightly {
      cargo = cargo.arg("+nightly");
    }
    cargo = cargo.args(["fmt", "--all"]);
    if mode == Mode::Check {
      cargo = cargo.arg("--check");
    }
    cargo = cargo.args(&self.args);
    if !self.args.iter().any(|a| a == "--") {
      cargo = cargo.arg("--");
    }
    cargo.arg("-l")
  }

  fn run(&self, shell: &mut Shell, mode: Mode) -> Result<Vec<PathBuf>> {
    let command = self.command(mode);
    let label = match self.channel {
      RustfmtChannel::Stable => "cargo fmt",
      RustfmtChannel::Nightly => "cargo +nightly fmt",
    };
    shell.status(mode.label(), format_args!("Rust sources ({label})"))?;
    shell.verbose(|s| s.status("Running", &command))?;
    let output = command.output_unchecked()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
      shell.err().write_all(stderr.as_bytes())?;
    }
    let files = self.listed_files(&String::from_utf8_lossy(&output.stdout));
    if mode == Mode::Write || files.is_empty() {
      command.check(output.status, &[])?;
    }
    Ok(files)
  }

  fn listed_files(&self, stdout: &str) -> Vec<PathBuf> {
    let mut files: Vec<_> = stdout
      .lines()
      .map(str::trim)
      .filter(|l| !l.is_empty())
      .map(|l| {
        let path = Path::new(l);
        path.strip_prefix(&self.root).unwrap_or(path).to_owned()
      })
      .collect();
    files.sort();
    files.dedup();
    files
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn formatters_follow_config_files_unless_set() {
    let (dir, project) =
      project("[project]\nlanguages = [\"c\"]\n\n[format.black]\nenabled = false\n");
    assert!(ToolFormatter::new(&project, Kind::Qmlformat).is_none());
    assert!(ToolFormatter::new(&project, Kind::ClangFormat).is_some());
    std::fs::write(dir.path().join(".qmlformat.ini"), "").unwrap();
    std::fs::write(dir.path().join(".stylua.toml"), "").unwrap();
    std::fs::write(dir.path().join("pyproject.toml"), "[tool.black]\n").unwrap();
    assert!(ToolFormatter::new(&project, Kind::Qmlformat).is_some());
    assert!(ToolFormatter::new(&project, Kind::Stylua).is_some());
    assert!(ToolFormatter::new(&project, Kind::Black).is_none());
  }

  #[test]
  fn commands_use_each_tools_own_flags() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"c\"]\n\n[tools]\nblack = \"25.9.0\"\nqmlformat = \"system\"\nclang-format = \"system\"\n\n[format.black]\nenabled = true\nargs = [\"-l\", \"100\"]\n\n[format.qmlformat]\nenabled = true\n\n[format.stylua]\nenabled = true\n",
    );
    let files = [PathBuf::from("a.py")];
    let command = |kind, mode| {
      ToolFormatter::new(&project, kind)
        .unwrap()
        .command(mode, &files)
        .to_string()
    };
    assert_eq!(
      command(Kind::Black, Mode::Check),
      "uv tool run --quiet --from black==25.9.0 black -l 100 --check -- a.py"
    );
    assert_eq!(command(Kind::Qmlformat, Mode::Write), "qmlformat -i a.py");
    assert_eq!(command(Kind::Qmlformat, Mode::Check), "qmlformat a.py");
    assert_eq!(command(Kind::Stylua, Mode::Write), "stylua -- a.py");
    assert_eq!(
      command(Kind::ClangFormat, Mode::Check),
      "clang-format --dry-run --Werror -- a.py"
    );
  }

  #[test]
  fn check_output_names_the_offending_files() {
    let root = Path::new("/work/p");
    let clang = "src/a.cpp:1:4: error: code should be clang-formatted [-Wclang-format-violations]\nint  main(){}\n   ^\nsrc/dir with space/b.h:10:1: warning: code should be clang-formatted [-Wclang-format-violations]\n";
    assert_eq!(
      Kind::ClangFormat.violations(root, clang),
      [PathBuf::from("src/a.cpp"), "src/dir with space/b.h".into()]
    );
    assert_eq!(
      Kind::ClangFormat.violations(root, "error: unknown argument\n"),
      Vec::<PathBuf>::new()
    );
    let black = "would reformat /work/p/scripts/a.py\nwould reformat b.py\nOh no! 💥 💔 💥\n2 files would be reformatted.\n";
    assert_eq!(
      Kind::Black.violations(root, black),
      [PathBuf::from("scripts/a.py"), "b.py".into()]
    );
    let stylua = "Diff in src/meta.lua:\n1 1    | -local x =  1\n";
    assert_eq!(
      Kind::Stylua.violations(root, stylua),
      [PathBuf::from("src/meta.lua")]
    );
    assert_eq!(
      Kind::Qmlformat.violations(root, black),
      Vec::<PathBuf>::new()
    );
  }

  #[test]
  fn writes_report_only_rewritten_files() {
    let dir = tempfile::tempdir().unwrap();
    let sources = vec![PathBuf::from("a"), PathBuf::from("b")];
    for name in &sources {
      std::fs::write(dir.path().join(name), "x").unwrap();
    }
    let before = fingerprints(dir.path(), &sources).unwrap();
    std::fs::write(dir.path().join("b"), "y").unwrap();
    let after = fingerprints(dir.path(), &sources).unwrap();
    assert_eq!(before[0], after[0]);
    assert_ne!(before[1], after[1]);
  }

  fn rustfmt(channel: RustfmtChannel, args: &[&str]) -> Rustfmt {
    Rustfmt {
      root: "/repo".into(),
      channel,
      args: args.iter().map(|&a| a.to_owned()).collect(),
    }
  }

  #[test]
  fn rustfmt_command_follows_channel_mode_and_args() {
    assert_eq!(
      rustfmt(RustfmtChannel::Stable, &[])
        .command(Mode::Write)
        .to_string(),
      "cargo fmt --all -- -l"
    );
    assert_eq!(
      rustfmt(RustfmtChannel::Nightly, &["--", "--edition", "2024"])
        .command(Mode::Check)
        .to_string(),
      "cargo +nightly fmt --all --check -- --edition 2024 -l"
    );
  }

  #[test]
  fn rustfmt_listed_files_are_relative_and_unique() {
    let files = rustfmt(RustfmtChannel::Stable, &[])
      .listed_files("/repo/src/b.rs\n/repo/src/a.rs\n\n/repo/src/a.rs\n/elsewhere/c.rs\n");
    assert_eq!(
      files,
      [
        Path::new("/elsewhere/c.rs"),
        Path::new("src/a.rs"),
        Path::new("src/b.rs")
      ]
    );
  }
}
