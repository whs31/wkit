#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRun {
  pub success: bool,
  pub output: String,
}

impl GitRun {
  pub fn from_output(output: &std::process::Output) -> Self {
    Self {
      success: output.status.success(),
      output: format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
      ),
    }
  }

  pub fn failure(message: impl Into<String>) -> Self {
    Self {
      success: false,
      output: message.into(),
    }
  }

  pub fn fatal(&self) -> Option<&str> {
    self
      .output
      .lines()
      .rev()
      .find_map(|line| line.strip_prefix("fatal: "))
  }
}
