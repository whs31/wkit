use crate::{Error, Result};

pub const RUSTFMT_CONFIGS: &[&str] = &["rustfmt.toml", ".rustfmt.toml"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum RustfmtChannel {
  Stable,
  Nightly,
}

impl RustfmtChannel {
  pub fn from_path(root: &std::path::Path) -> Result<Self> {
    for name in RUSTFMT_CONFIGS {
      let path = root.join(name);
      let Some(text) = crate::fs::read_optional(&path)? else {
        continue;
      };
      let table: toml::Table = toml::from_str(&text).map_err(|source| Error::Toml {
        path,
        source: Box::new(source),
      })?;
      let unstable = table
        .get("unstable_features")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
      return Ok(if unstable {
        Self::Nightly
      } else {
        Self::Stable
      });
    }
    Ok(Self::Stable)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn follows_unstable_features() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rustfmt.toml");
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Stable
    );
    std::fs::write(&config, "tab_spaces = 2\n").unwrap();
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Stable
    );
    std::fs::write(
      &config,
      "unstable_features = true\nimports_granularity = \"Crate\"\n",
    )
    .unwrap();
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Nightly
    );
  }

  #[test]
  fn hidden_config_is_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
      dir.path().join(".rustfmt.toml"),
      "unstable_features = true\n",
    )
    .unwrap();
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Nightly
    );
  }

  #[test]
  fn invalid_config_is_reported_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rustfmt.toml"), "tab_spaces = \n").unwrap();
    let err = RustfmtChannel::from_path(dir.path()).unwrap_err();
    assert!(matches!(err, Error::Toml { ref path, .. } if path.ends_with("rustfmt.toml")));
  }
}
