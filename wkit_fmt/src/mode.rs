#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
  #[default]
  Write,

  Check,
}

impl Mode {
  pub fn label(self) -> &'static str {
    match self {
      Self::Write => "Formatting",
      Self::Check => "Checking",
    }
  }
}
