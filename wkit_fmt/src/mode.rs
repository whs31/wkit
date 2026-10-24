#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
  #[default]
  Write,

  Check,
}
