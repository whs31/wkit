#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CleanSection {
  pub paths: Option<Vec<String>>,
  pub extra: Vec<String>,
}

impl CleanSection {
  pub const DEFAULT_PATHS: &[&str] = &[
    "build",
    "cmake-build-*",
    "cmake-visual-studio-*",
    ".cache",
    "graph.html",
    "docs/graph.html",
    "compile_commands.json",
    "CMakeUserPresets.json",
    "conan_provider.cmake",
  ];

  pub fn effective_paths(
    &self,
    languages: &std::collections::BTreeSet<wkit_core::Language>,
  ) -> Vec<String> {
    let mut paths = self.paths.clone().unwrap_or_else(|| {
      let mut defaults: Vec<String> = Self::DEFAULT_PATHS.iter().map(|&s| s.to_owned()).collect();
      if languages.contains(&wkit_core::Language::Rust) {
        defaults.push("target".to_owned());
      }
      defaults
    });
    paths.extend(self.extra.iter().cloned());
    paths
  }
}

#[cfg(test)]
mod tests {
  use wkit_core::Language;

  use super::*;

  #[test]
  fn defaults_follow_languages_and_extra_is_appended() {
    let clean = CleanSection {
      paths: None,
      extra: vec!["run/out".into()],
    };
    let rust = clean.effective_paths(&[Language::Rust].into());
    assert!(rust.contains(&"target".into()) && rust.ends_with(&["run/out".into()]));
    assert!(
      !clean
        .effective_paths(&[Language::Cpp].into())
        .contains(&"target".into())
    );
  }

  #[test]
  fn explicit_paths_replace_defaults() {
    let clean = CleanSection {
      paths: Some(vec!["out".into()]),
      extra: vec![],
    };
    assert_eq!(clean.effective_paths(&[Language::Rust].into()), ["out"]);
  }
}
