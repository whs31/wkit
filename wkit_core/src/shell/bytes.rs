#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Bytes(pub u64);

impl std::fmt::Display for Bytes {
  #[allow(clippy::cast_precision_loss)]
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = self.0 as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
      value /= 1024.0;
      unit += 1;
    }
    if unit == 0 {
      write!(f, "{}{}", self.0, UNITS[0])
    } else {
      write!(f, "{value:.1}{}", UNITS[unit])
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scales_to_binary_units() {
    assert_eq!(Bytes(0).to_string(), "0B");
    assert_eq!(Bytes(1023).to_string(), "1023B");
    assert_eq!(Bytes(1536).to_string(), "1.5KiB");
    assert_eq!(Bytes(3 * 1024 * 1024 * 1024).to_string(), "3.0GiB");
  }
}
