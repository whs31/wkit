#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
  Required,
  Recommended,
  Optional,
}

impl Need {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Required => "required",
      Self::Recommended => "recommended",
      Self::Optional => "optional",
    }
  }

  pub fn when(required: bool) -> Self {
    if required {
      Self::Required
    } else {
      Self::Optional
    }
  }
}
