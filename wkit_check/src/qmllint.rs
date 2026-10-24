use wkit_core::{Shell, fs::SourceSet, tool::Tool};
use wkit_manifest::check::QmllintCheck;

use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct Qmllint {
  root: std::path::PathBuf,
  tool: Tool,
  config: QmllintCheck,
}

impl Qmllint {
  pub const TOOL: &str = "qmllint";

  pub fn new(root: impl Into<std::path::PathBuf>, tool: Tool, config: QmllintCheck) -> Self {
    Self {
      root: root.into(),
      tool,
      config,
    }
  }

  pub fn sources(&self) -> Result<Vec<std::path::PathBuf>> {
    let exclude = self.config.exclude.clone().unwrap_or_else(|| {
      SourceSet::DEFAULT_EXCLUDES
        .iter()
        .map(|&e| e.to_owned())
        .collect()
    });
    Ok(
      SourceSet::new(&self.root)
        .extensions(["qml"])
        .include(&self.config.include)
        .exclude(exclude)
        .collect()?
        .into_iter()
        .map(|p| {
          p.strip_prefix(&self.root)
            .map(std::path::Path::to_owned)
            .unwrap_or(p)
        })
        .collect(),
    )
  }

  pub fn command(&self, sources: &[std::path::PathBuf]) -> wkit_core::ProcessBuilder {
    self
      .tool
      .command()
      .cwd(&self.root)
      .args(&self.config.args)
      .args(sources)
  }
}

impl Check for Qmllint {
  fn name(&self) -> String {
    "qmllint".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    let sources = self.sources()?;
    if sources.is_empty() {
      return Ok(Outcome::skipped("no QML files"));
    }
    let command = self.command(&sources);
    shell.status(
      "Checking",
      format_args!(
        "{} with {}",
        Outcome::plural(sources.len(), "QML file"),
        self.tool
      ),
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
        Outcome::plural(sources.len(), "file")
      )));
    }
    Ok(Outcome::failed(
      if warnings == 0 {
        format!("qmllint failed ({})", output.status)
      } else {
        Outcome::plural(warnings, "warning")
      },
      text.lines().map(|l| format!("  {l}")).collect(),
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lints_qml_sources_outside_build_outputs() {
    let dir = tempfile::tempdir().unwrap();
    for file in ["qml/Main.qml", "build/Debug/gen.qml", "qml/readme.md"] {
      let path = dir.path().join(file);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, "").unwrap();
    }
    let lint = Qmllint::new(
      dir.path(),
      Tool::system(Qmllint::TOOL),
      QmllintCheck {
        args: vec!["-I".into(), "build/Release".into()],
        ..QmllintCheck::default()
      },
    );
    let sources = lint.sources().unwrap();
    assert_eq!(sources, [std::path::PathBuf::from("qml/Main.qml")]);
    assert_eq!(
      lint.command(&sources).to_string(),
      "qmllint -I build/Release qml/Main.qml"
    );
  }
}
