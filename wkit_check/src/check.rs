use wkit_core::Shell;

use crate::{CheckOptions, Outcome, Result};

pub trait Check: std::fmt::Debug {
  fn name(&self) -> String;

  fn run(&self, shell: &mut Shell, options: &CheckOptions) -> Result<Outcome>;
}
