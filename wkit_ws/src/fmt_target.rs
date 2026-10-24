use wkit_core::ProcessBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmtTarget {
  Wkit,
  Just {
    justfile: std::path::PathBuf,
    recipe: String,
  },
}

impl FmtTarget {
  pub const RECIPES: &[&str] = &["fmt", "format"];

  pub fn detect(dir: &std::path::Path) -> Option<Self> {
    if dir.join("wkit.toml").is_file() {
      return Some(Self::Wkit);
    }
    let justfile = std::fs::read_dir(dir)
      .ok()?
      .flatten()
      .map(|entry| entry.path())
      .find(|path| {
        path.is_file()
          && path.file_name().is_some_and(|n| {
            matches!(
              n.to_string_lossy().to_lowercase().as_str(),
              "justfile" | ".justfile"
            )
          })
      })?;
    let output = Self::just(&justfile)
      .args(["--dump", "--dump-format", "json"])
      .output()
      .ok()?;
    let dump: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let defined =
      |name: &str| dump["recipes"].get(name).is_some() || dump["aliases"].get(name).is_some();
    let recipe = Self::RECIPES.iter().find(|r| defined(r))?;
    Some(Self::Just {
      justfile,
      recipe: (*recipe).to_owned(),
    })
  }

  pub fn command(&self, dir: &std::path::Path, wkit: &std::path::Path) -> ProcessBuilder {
    match self {
      Self::Wkit => ProcessBuilder::new(wkit)
        .arg("fmt")
        .cwd(dir)
        .env("WKIT_COLOR", "never"),
      Self::Just { justfile, recipe } => Self::just(justfile).arg(recipe),
    }
  }

  pub fn label(&self) -> String {
    match self {
      Self::Wkit => "wkit fmt".to_owned(),
      Self::Just { recipe, .. } => format!("just {recipe}"),
    }
  }

  fn just(justfile: &std::path::Path) -> ProcessBuilder {
    let dir = justfile.parent().unwrap_or(justfile);
    ProcessBuilder::new("just")
      .arg("--justfile")
      .arg(justfile)
      .arg("--working-directory")
      .arg(dir)
      .cwd(dir)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn wkit_projects_format_with_wkit() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(FmtTarget::detect(dir.path()), None);
    std::fs::write(dir.path().join("wkit.toml"), "").unwrap();
    let target = FmtTarget::detect(dir.path()).unwrap();
    assert_eq!(target.label(), "wkit fmt");
    assert_eq!(
      target
        .command(dir.path(), std::path::Path::new("/bin/wkit"))
        .to_string(),
      "/bin/wkit fmt"
    );
  }
}
