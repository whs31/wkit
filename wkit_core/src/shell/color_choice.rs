#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, derive_more::Display)]
#[display(rename_all = "lowercase")]
pub enum ColorChoice {
  #[default]
  Auto,
  Always,
  Never,
}

impl ColorChoice {
  pub const ALL: [Self; 3] = [Self::Auto, Self::Always, Self::Never];

  pub(super) fn to_stream(self) -> anstream::ColorChoice {
    match self {
      Self::Auto => anstream::ColorChoice::Auto,
      Self::Always => anstream::ColorChoice::Always,
      Self::Never => anstream::ColorChoice::Never,
    }
  }
}

impl std::str::FromStr for ColorChoice {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::ALL
      .into_iter()
      .find(|choice| choice.to_string() == s)
      .ok_or_else(|| {
        format!("argument for --color must be auto, always, or never, but found `{s}`")
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_cli_values() {
    assert_eq!("never".parse::<ColorChoice>(), Ok(ColorChoice::Never));
    assert_eq!("auto".parse::<ColorChoice>(), Ok(ColorChoice::Auto));
    assert!("sometimes".parse::<ColorChoice>().is_err());
  }
}
