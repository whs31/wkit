#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Elapsed(pub std::time::Duration);

impl std::fmt::Display for Elapsed {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let secs = self.0.as_secs();
    if secs >= 60 {
      write!(f, "{}m {:02}s", secs / 60, secs % 60)
    } else {
      write!(f, "{:.2}s", self.0.as_secs_f64())
    }
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::*;

  #[test]
  fn formats_like_cargo() {
    assert_eq!(Elapsed(Duration::from_millis(1234)).to_string(), "1.23s");
    assert_eq!(Elapsed(Duration::from_secs(125)).to_string(), "2m 05s");
  }
}
