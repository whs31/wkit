#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum TestRunner {
  Cargo,
  Nextest,
  Ctest,
  Catch2,
}

impl TestRunner {
  pub fn serves(self, language: wkit_core::Language) -> bool {
    match self {
      Self::Cargo | Self::Nextest => language == wkit_core::Language::Rust,
      Self::Ctest | Self::Catch2 => language.is_c_family(),
    }
  }
}

#[cfg(test)]
mod tests {
  use wkit_core::Language;

  use super::*;

  #[test]
  fn runners_serve_their_language_family() {
    assert!(TestRunner::Nextest.serves(Language::Rust));
    assert!(!TestRunner::Nextest.serves(Language::Cpp));
    assert!(TestRunner::Catch2.serves(Language::C));
    assert_eq!(TestRunner::Catch2.to_string(), "catch2");
  }
}
