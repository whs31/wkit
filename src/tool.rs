use crate::ProcessBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
  Uv { package: String, version: String },
  Path(std::path::PathBuf),
  System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
  pub name: String,
  pub source: ToolSource,
}

impl Tool {
  #[cfg(test)]
  pub fn pinned(name: impl Into<String>, version: impl Into<String>) -> Self {
    let name = name.into();
    Self {
      source: ToolSource::Uv {
        package: name.clone(),
        version: version.into(),
      },
      name,
    }
  }

  #[cfg(test)]
  pub fn system(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      source: ToolSource::System,
    }
  }

  pub fn version(&self) -> Option<&str> {
    match &self.source {
      ToolSource::Uv { version, .. } => Some(version),
      ToolSource::Path(_) | ToolSource::System => None,
    }
  }

  pub fn command(&self) -> ProcessBuilder {
    match &self.source {
      ToolSource::Uv { package, version } => ProcessBuilder::new("uv")
        .args(["tool", "run", "--quiet", "--from"])
        .arg(format!("{package}=={version}"))
        .arg(&self.name),
      ToolSource::Path(path) => ProcessBuilder::new(path),
      ToolSource::System => ProcessBuilder::new(&self.name),
    }
  }
}

impl std::fmt::Display for Tool {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match &self.source {
      ToolSource::Uv { version, .. } => write!(f, "{} {version}", self.name),
      ToolSource::Path(path) => write!(f, "{} ({})", self.name, path.display()),
      ToolSource::System => write!(f, "{} (system)", self.name),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pinned_tools_run_through_uv() {
    let tool = Tool::pinned("clang-format", "23.1.3");
    assert_eq!(
      tool.command().arg("--version").to_string(),
      "uv tool run --quiet --from clang-format==23.1.3 clang-format --version"
    );
    assert_eq!(tool.to_string(), "clang-format 23.1.3");
    assert_eq!(tool.version(), Some("23.1.3"));
  }

  #[test]
  fn path_and_system_tools_run_directly() {
    let path = Tool {
      name: "clang-tidy".into(),
      source: ToolSource::Path("/opt/llvm/bin/clang-tidy".into()),
    };
    assert_eq!(path.command().to_string(), "/opt/llvm/bin/clang-tidy");
    assert_eq!(Tool::system("qmllint").command().to_string(), "qmllint");
    assert_eq!(Tool::system("qmllint").to_string(), "qmllint (system)");
  }
}
