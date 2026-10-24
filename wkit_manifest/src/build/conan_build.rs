use super::ConanValue;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanBuild {
  pub profile_host: Option<String>,
  pub profile_build: Option<String>,
  pub settings: std::collections::BTreeMap<String, String>,
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
      conf: std::collections::BTreeMap::new(),
      build_missing: true,
      test_option: None,
      workspace: false,
      args: Vec::new(),
    }
  }
}

impl ConanBuild {
  pub const TEST_OPTIONS: [&str; 6] = [
    "test",
    "tests",
    "with_tests",
    "build_tests",
    "enable_tests",
    "testing",
  ];

  pub fn setting_args(&self) -> Vec<String> {
    self
      .settings
      .iter()
      .flat_map(|(key, value)| ["-s".to_owned(), format!("{key}={value}")])
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
  fn settings_and_conf_become_arguments() {
    let conan: ConanBuild = toml::from_str(
      r#"
      settings = { "compiler.cppstd" = "gnu23" }
      conf = { "tools.build:jobs" = 4, "core:non_interactive" = true }
      "#,
    )
    .unwrap();
    assert_eq!(conan.setting_args(), ["-s", "compiler.cppstd=gnu23"]);
    assert_eq!(
      conan.conf_args(),
      [
        "-c",
        "core:non_interactive=True",
        "-c",
        "tools.build:jobs=4"
      ]
    );
    assert_eq!(conan.test_option, None);
    assert!(conan.build_missing);
  }
}
