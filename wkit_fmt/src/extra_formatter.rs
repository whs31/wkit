use wkit_core::{Error, ProcessBuilder, Result, Shell, fs::SourceSet, tool::Tool};
use wkit_manifest::Project;

use crate::{Batches, ExtraKind, Mode, Snapshot};

#[derive(Debug, Clone)]
pub struct ExtraFormatter {
  kind: ExtraKind,
  root: std::path::PathBuf,
  tool: Tool,
  sources: SourceSet,
  args: Vec<String>,
}

impl ExtraFormatter {
  pub const BATCH: usize = 64;

  pub fn new(project: &Project, kind: ExtraKind) -> Option<Self> {
    let config = kind.config(&project.manifest);
    if !config
      .enabled
      .unwrap_or_else(|| kind.is_configured(&project.root))
    {
      return None;
    }
    let extensions = config.extensions.clone().unwrap_or_else(|| {
      kind
        .default_extensions()
        .iter()
        .map(|&e| e.to_owned())
        .collect()
    });
    let exclude = config.exclude.clone().unwrap_or_else(|| {
      SourceSet::DEFAULT_EXCLUDES
        .iter()
        .map(|&e| e.to_owned())
        .collect()
    });
    Some(Self {
      kind,
      root: project.root.clone(),
      tool: project.tool(kind.tool()),
      sources: SourceSet::new(&project.root)
        .extensions(extensions)
        .include(&config.include)
        .exclude(exclude),
      args: config.args.clone(),
    })
  }

  pub fn kind(&self) -> ExtraKind {
    self.kind
  }

  pub fn tool(&self) -> &Tool {
    &self.tool
  }

  pub fn command(&self, mode: Mode, batch: &[std::path::PathBuf]) -> ProcessBuilder {
    let command = self.tool.command().cwd(&self.root).args(&self.args);
    let command = match (mode, self.kind) {
      (Mode::Write, kind) => command.args(kind.write_flags()),
      (Mode::Check, ExtraKind::Qmlformat) => command,
      (Mode::Check, _) => command.arg("--check"),
    };
    match self.kind {
      ExtraKind::Qmlformat => command.args(batch),
      ExtraKind::Black | ExtraKind::Stylua => command.arg("--").args(batch),
    }
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
      shell.verbose(|s| {
        s.note(format_args!(
          "no {} sources to format",
          self.kind.language()
        ))
      })?;
      return Ok(Vec::new());
    }
    self.tool.command().arg("--version").output()?;
    shell.status(
      mode.label(),
      format_args!(
        "{} {} file{} with {}",
        sources.len(),
        self.kind.language(),
        if sources.len() == 1 { "" } else { "s" },
        self.tool
      ),
    )?;
    let progress = shell.progress(mode.label(), sources.len() as u64);
    let before = match mode {
      Mode::Write => Some(Snapshot::take(&self.root, &sources)?),
      Mode::Check => None,
    };
    let flagged =
      Batches::new(Self::BATCH, jobs).run(&sources, &progress, |batch| self.run_batch(mode, batch));
    progress.finish_and_clear();
    let flagged = flagged?;
    match before {
      Some(before) => before.changed(&self.root, &sources),
      None => Ok(flagged),
    }
  }

  fn run_batch(&self, mode: Mode, batch: &[std::path::PathBuf]) -> Result<Vec<std::path::PathBuf>> {
    if mode == Mode::Check && self.kind == ExtraKind::Qmlformat {
      return self.compare_each(batch);
    }
    let command = self.command(mode, batch);
    let output = command.output_unchecked()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let flagged = match mode {
      Mode::Write => Vec::new(),
      Mode::Check => self
        .kind
        .violations(&self.root, &format!("{stdout}\n{stderr}")),
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

  fn compare_each(&self, batch: &[std::path::PathBuf]) -> Result<Vec<std::path::PathBuf>> {
    let mut flagged = Vec::new();
    for path in batch {
      let formatted = self
        .command(Mode::Check, std::slice::from_ref(path))
        .output()?;
      let full = self.root.join(path);
      let current = std::fs::read(&full).map_err(|source| Error::Read { path: full, source })?;
      if formatted.stdout != current {
        flagged.push(path.clone());
      }
    }
    Ok(flagged)
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
    assert!(ExtraFormatter::new(&project, ExtraKind::Qmlformat).is_none());
    std::fs::write(dir.path().join(".qmlformat.ini"), "").unwrap();
    std::fs::write(dir.path().join("pyproject.toml"), "[tool.black]\n").unwrap();
    assert!(ExtraFormatter::new(&project, ExtraKind::Qmlformat).is_some());
    assert!(ExtraFormatter::new(&project, ExtraKind::Black).is_none());
  }

  #[test]
  fn commands_use_each_tools_own_check_flag() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"c\"]\n\n[tools]\nblack = \"25.9.0\"\nqmlformat = \"system\"\n\n[format.black]\nenabled = true\nargs = [\"-l\", \"100\"]\n\n[format.qmlformat]\nenabled = true\n\n[format.stylua]\nenabled = true\n",
    );
    let files = [std::path::PathBuf::from("a.py")];
    let black = ExtraFormatter::new(&project, ExtraKind::Black).unwrap();
    assert_eq!(
      black.command(Mode::Check, &files).to_string(),
      "uv tool run --quiet --from black==25.9.0 black -l 100 --check -- a.py"
    );
    let qml = ExtraFormatter::new(&project, ExtraKind::Qmlformat).unwrap();
    assert_eq!(
      qml.command(Mode::Write, &files).to_string(),
      "qmlformat -i a.py"
    );
    assert_eq!(
      qml.command(Mode::Check, &files).to_string(),
      "qmlformat a.py"
    );
    let stylua = ExtraFormatter::new(&project, ExtraKind::Stylua).unwrap();
    assert_eq!(
      stylua.command(Mode::Write, &files).to_string(),
      "stylua -- a.py"
    );
  }
}
