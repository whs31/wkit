#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("`{}` has {found}, expected {expected}", path.display())]
pub struct Mismatch {
  pub path: std::path::PathBuf,
  pub found: String,
  pub expected: String,
}
