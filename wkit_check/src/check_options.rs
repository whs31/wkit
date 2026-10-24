#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOptions {
  pub jobs: usize,
  pub cache: bool,
  pub build_dir: Option<std::path::PathBuf>,
  pub verbose: bool,
}

impl Default for CheckOptions {
  fn default() -> Self {
    Self {
      jobs: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
      cache: true,
      build_dir: None,
      verbose: false,
    }
  }
}
