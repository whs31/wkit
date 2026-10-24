#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
  pub altitude: f64,
  pub speed: f64,
}

impl Sample {
  pub fn parse(line: &str) -> Option<Self> {
    let (altitude, speed) = line.trim().split_once(',')?;
    Some(Self {
      altitude: altitude.trim().parse().ok()?,
      speed: speed.trim().parse().ok()?,
    })
  }
}

pub fn max_altitude(samples: &[Sample]) -> Option<f64> {
  samples.iter().map(|s| s.altitude).reduce(f64::max)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_csv_lines() {
    assert_eq!(
      Sample::parse(" 120.5, 14 "),
      Some(Sample {
        altitude: 120.5,
        speed: 14.0
      })
    );
    assert_eq!(Sample::parse("garbage"), None);
  }

  #[test]
  fn finds_the_highest_sample() {
    let samples = [
      Sample::parse("10,1").unwrap(),
      Sample::parse("30,2").unwrap(),
    ];
    assert_eq!(max_altitude(&samples), Some(30.0));
    assert_eq!(max_altitude(&[]), None);
  }
}
