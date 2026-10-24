#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaValue {
  Literal(String),
  Dynamic(String),
  Unresolved(String),
}

impl MetaValue {
  pub fn literal(&self) -> Option<&str> {
    match self {
      Self::Literal(value) => Some(value),
      Self::Dynamic(_) | Self::Unresolved(_) => None,
    }
  }
}
