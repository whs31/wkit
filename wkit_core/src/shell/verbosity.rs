#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
  Quiet,

  #[default]
  Normal,

  Verbose,
}
