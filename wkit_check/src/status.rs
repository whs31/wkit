#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum Status {
  #[display("ok")]
  Passed,

  #[display("FAILED")]
  Failed,

  #[display("skipped")]
  Skipped,
}

impl Status {
  pub fn style(self) -> &'static wkit_core::shell::style::Style {
    match self {
      Self::Passed => &wkit_core::shell::style::HEADER,
      Self::Failed => &wkit_core::shell::style::ERROR,
      Self::Skipped => &wkit_core::shell::style::DIM,
    }
  }
}
