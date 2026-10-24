use wkit_ws::Runner;

#[derive(Debug, Clone, Copy, clap::Args)]
pub struct Jobs {
  /// Number of projects to process in parallel
  #[arg(short, long, value_name = "N", default_value_t = Runner::DEFAULT_JOBS)]
  pub jobs: usize,
}

impl Jobs {
  pub fn runner(self) -> Runner {
    Runner::new(self.jobs)
  }
}

#[derive(Debug, Clone, Copy, clap::Args)]
pub struct Publish {
  /// Commit the result
  #[arg(long)]
  pub commit: bool,

  /// Push the commit as well
  #[arg(long, requires = "commit")]
  pub push: bool,
}
