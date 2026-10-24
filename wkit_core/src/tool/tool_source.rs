#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
  Uv { package: String, version: String },
  Path(std::path::PathBuf),
  System,
}
