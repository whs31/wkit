use super::Manifest;

#[derive(Debug, Clone)]
pub struct Parsed {
  pub manifest: Manifest,
  pub warnings: Vec<String>,
}
