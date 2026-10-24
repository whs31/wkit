#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
  pub path: std::path::PathBuf,
  pub from: String,
  pub to: String,
  pub(crate) abs: std::path::PathBuf,
  pub(crate) contents: String,
}

impl Change {
  pub fn abs_path(&self) -> &std::path::Path {
    &self.abs
  }

  pub fn contents(&self) -> &str {
    &self.contents
  }

  pub fn apply(&self) -> wkit_core::Result<()> {
    wkit_core::fs::write(&self.abs, &self.contents)
  }
}
