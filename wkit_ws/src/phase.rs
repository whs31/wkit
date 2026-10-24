#[derive(Debug, Clone)]
pub struct Phase {
  bar: indicatif::ProgressBar,
  path: String,
}

impl Phase {
  pub fn new(bar: indicatif::ProgressBar, path: impl Into<String>) -> Self {
    Self {
      bar,
      path: path.into(),
    }
  }

  pub fn hidden() -> Self {
    Self::new(indicatif::ProgressBar::hidden(), "")
  }

  pub fn set(&self, phase: &str, fraction: Option<f64>) {
    let message = match fraction {
      #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
      Some(f) => format!(" {}: {phase} {}%", self.path, (f * 100.0).round() as u32),
      None => format!(" {}: {phase}", self.path),
    };
    self.bar.set_message(message);
  }
}
