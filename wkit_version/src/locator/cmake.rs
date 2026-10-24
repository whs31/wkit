use wkit_core::version::Flavor;

use super::Locator;
use crate::Slot;

impl Locator<'_> {
  pub(super) fn cmake(text: &str, flavor: Flavor) -> Option<Slot> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"(?ms)^[ \t]*(?i:project)[ \t]*\([^)]*?\bVERSION\s+(?<version>[^\s)]+)")
        .expect("valid")
    });
    Self::first_match(&RE, text).map(|span| Slot::version(span, flavor))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn find(text: &str) -> Option<&str> {
    Locator::cmake(text, Flavor::Numeric).map(|s| &text[s.span])
  }

  #[test]
  fn finds_the_project_version() {
    assert_eq!(
      find("project(mms VERSION 3.12.0 LANGUAGES CXX)"),
      Some("3.12.0")
    );
    assert_eq!(
      find(
        "cmake_minimum_required(VERSION 3.25)\n\nproject(rcommon_lua\n  VERSION 1.4.2\n  LANGUAGES CXX\n)\n"
      ),
      Some("1.4.2")
    );
    assert_eq!(find("PROJECT(x VERSION 2.0.0)"), Some("2.0.0"));
  }

  #[test]
  fn ignores_other_commands() {
    assert_eq!(
      find("cmake_minimum_required(VERSION 3.25)\nproject(x LANGUAGES C)\n"),
      None
    );
    assert_eq!(find("qt_standard_project_setup(VERSION 6.5)\n"), None);
    assert_eq!(find("# project(x VERSION 1.0.0)\n"), None);
  }
}
