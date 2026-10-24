use wkit_core::version::Flavor;

use super::Locator;
use crate::Slot;

impl Locator<'_> {
  pub(super) fn conan(text: &str, flavor: Flavor) -> Option<Slot> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r#"(?m)^[ \t]*version[ \t]*=[ \t]*["'](?<version>[^"'\n]*)["']"#)
        .expect("valid")
    });
    Self::first_match(&RE, text).map(|span| Slot::version(span, flavor))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_class_attribute() {
    let text = "class Pkg(ConanFile):\n    name = \"mms\"\n    version = \"3.12.0\"\n\n    def configure(self):\n        version = str(self.version)\n";
    let slot = Locator::conan(text, Flavor::Full).unwrap();
    assert_eq!(&text[slot.span], "3.12.0");
    assert!(Locator::conan("class Pkg(ConanFile):\n    name = 'x'\n", Flavor::Full).is_none());
  }
}
