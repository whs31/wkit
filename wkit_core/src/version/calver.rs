use super::{Date, VersionError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalVer {
  pub date: Date,
  pub micro: Option<u32>,
}

impl CalVer {
  pub fn on(date: Date) -> Self {
    Self { date, micro: None }
  }

  #[must_use]
  pub fn next(&self, today: Date) -> Self {
    if self.date < today {
      Self::on(today)
    } else {
      Self {
        date: self.date,
        micro: Some(self.micro.map_or(1, |m| m + 1)),
      }
    }
  }
}

impl std::str::FromStr for CalVer {
  type Err = VersionError;

  fn from_str(input: &str) -> Result<Self, Self::Err> {
    let invalid = || VersionError::InvalidCalver {
      input: input.to_owned(),
    };
    let parts: Vec<&str> = input.split('.').collect();
    let widths_ok =
      matches!(parts.as_slice(), [y, m, d, ..] if y.len() == 4 && m.len() == 2 && d.len() == 2);
    if !widths_ok
      || parts.len() > 4
      || parts
        .iter()
        .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
      return Err(invalid());
    }
    let year: i16 = parts[0].parse().map_err(|_| invalid())?;
    let month: i8 = parts[1].parse().map_err(|_| invalid())?;
    let day: i8 = parts[2].parse().map_err(|_| invalid())?;
    let date = Date::new(year, month, day).map_err(|_| invalid())?;
    let micro = match parts.get(3) {
      None => None,
      Some(m) if m.starts_with('0') => return Err(invalid()),
      Some(m) => Some(m.parse().map_err(|_| invalid())?),
    };
    Ok(Self { date, micro })
  }
}

impl std::fmt::Display for CalVer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{:04}.{:02}.{:02}",
      self.date.year(),
      self.date.month(),
      self.date.day()
    )?;
    if let Some(micro) = self.micro {
      write!(f, ".{micro}")?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use jiff::civil::date;

  use super::*;

  #[test]
  fn parses_and_displays() {
    let v: CalVer = "2026.10.03".parse().unwrap();
    assert_eq!(v.to_string(), "2026.10.03");
    let v: CalVer = "2026.10.03.2".parse().unwrap();
    assert_eq!(v.micro, Some(2));
    for bad in [
      "2026.10.3",
      "2026.13.01",
      "2026.02.30",
      "2026.10.03.0",
      "2026.10.03.01",
      "2026.10",
      "26.10.03",
      "2026.10.03.1.1",
    ] {
      assert!(bad.parse::<CalVer>().is_err(), "{bad}");
    }
  }

  #[test]
  fn next_is_today_or_the_next_micro() {
    let today = date(2026, 10, 24);
    let next = |s: &str| s.parse::<CalVer>().unwrap().next(today).to_string();
    assert_eq!(next("2026.10.03"), "2026.10.24");
    assert_eq!(next("2026.10.24"), "2026.10.24.1");
    assert_eq!(next("2026.10.24.4"), "2026.10.24.5");
    assert_eq!(next("2026.11.01"), "2026.11.01.1");
  }
}
