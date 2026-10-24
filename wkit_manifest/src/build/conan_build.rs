use super::ConanValue;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanBuild {
  pub profile_host: Option<String>,
  pub profile_build: Option<String>,
  pub settings: std::collections::BTreeMap<String, String>,
  pub options: std::collections::BTreeMap<String, ConanValue>,
  pub conf: std::collections::BTreeMap<String, ConanValue>,
  pub build_missing: bool,
  pub test_option: Option<String>,
  pub workspace: bool,
  pub args: Vec<String>,
}

impl Default for ConanBuild {
  fn default() -> Self {
    Self {
      profile_host: None,
      profile_build: None,
      settings: std::collections::BTreeMap::new(),
      options: std::collections::BTreeMap::new(),
      conf: std::collections::BTreeMap::new(),
      build_missing: true,
      test_option: Some("test".to_owned()),
      workspace: false,
      args: Vec::new(),
    }
  }
}

impl ConanBuild {
  pub fn option_args(&self) -> Vec<String> {
    self
      .options
      .iter()
      .flat_map(|(key, value)| {
        let scoped = if key.contains(':') {
          key.clone()
        } else {
          format!("&:{key}")
        };
        ["-o".to_owned(), format!("{scoped}={value}")]
      })
      .collect()
  }

  pub fn conf_args(&self) -> Vec<String> {
    self
      .conf
      .iter()
      .flat_map(|(key, value)| ["-c".to_owned(), format!("{key}={value}")])
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn options_are_consumer_scoped_with_python_bools() {
    let conan: ConanBuild = toml::from_str(
      r#"
      options = { test = true, with_map = false, "qt/*:shared" = true, jobs = 4 }
      conf = { "tools.build:jobs" = 4, "core:non_interactive" = true }
      "#,
    )
    .unwrap();
    assert_eq!(
      conan.option_args(),
      [
        "-o",
        "&:jobs=4",
        "-o",
        "qt/*:shared=True",
        "-o",
        "&:test=True",
        "-o",
        "&:with_map=False"
      ]
    );
    assert_eq!(
      conan.conf_args(),
      [
        "-c",
        "core:non_interactive=True",
        "-c",
        "tools.build:jobs=4"
      ]
    );
    assert_eq!(conan.test_option.as_deref(), Some("test"));
    assert!(conan.build_missing);
  }
}
