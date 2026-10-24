use std::collections::BTreeSet;

use serde::Deserialize;
use wkit_core::Language;

pub const DEFAULT_CLEAN: &[&str] = &[
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CleanSection {
  pub paths: Option<Vec<String>>,
  pub extra: Vec<String>,
}

impl CleanSection {
  pub fn effective_paths(&self, languages: &BTreeSet<Language>) -> Vec<String> {
    let mut paths = self.paths.clone().unwrap_or_else(|| {
      let mut defaults: Vec<String> = DEFAULT_CLEAN.iter().map(|&s| s.to_owned()).collect();
      if languages.contains(&Language::Rust) {
        defaults.push("target".to_owned());
      }
      defaults
    });
    paths.extend(self.extra.iter().cloned());
    paths
  }
}
