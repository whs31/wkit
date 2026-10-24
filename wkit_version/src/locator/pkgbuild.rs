use wkit_core::version::Flavor;

use super::Locator;
use crate::Slot;

impl Locator<'_> {
  pub(super) fn pkgbuild(text: &str, flavor: Flavor) -> Vec<Slot> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r#"(?m)^(?<key>_source_version|pkgver|pkgrel)=["']?(?<version>[^"'\s#]*)"#)
        .expect("valid")
    });
    let mut source = None;
    let mut pkgver = None;
    let mut pkgrel = None;
    for captures in RE.captures_iter(text) {
      let value = &captures["version"];
      if value.is_empty() || value.contains('$') {
        continue;
      }
      let span = captures
        .name("version")
        .expect("group always matches")
        .range();
      let target = match &captures["key"] {
        "_source_version" => &mut source,
        "pkgver" => &mut pkgver,
        _ => &mut pkgrel,
      };
      target.get_or_insert(span);
    }
    let mut slots: Vec<Slot> = source
      .map(|span| Slot::version(span, Flavor::Full))
      .into_iter()
      .chain(pkgver.map(|span| Slot::version(span, flavor)))
      .collect();
    if !slots.is_empty()
      && let Some(span) = pkgrel
    {
      slots.push(Slot::literal(span, "1"));
    }
    slots
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::SlotValue;

  #[test]
  fn finds_source_version_pkgver_and_pkgrel() {
    let text = "pkgname=xlink\n_source_version=1.2.0-rc.1\npkgver=1.2.0rc.1\npkgrel=3\n";
    let slots = Locator::pkgbuild(text, Flavor::PkgVer);
    let found: Vec<_> = slots
      .iter()
      .map(|s| (&text[s.span.clone()], s.value))
      .collect();
    assert_eq!(
      found,
      [
        ("1.2.0-rc.1", SlotValue::Version(Flavor::Full)),
        ("1.2.0rc.1", SlotValue::Version(Flavor::PkgVer)),
        ("3", SlotValue::Literal("1")),
      ]
    );
  }

  #[test]
  fn skips_computed_values() {
    let text = "_source_version='2.0.0'\npkgver=${_source_version//-/}\npkgrel=1\n";
    let slots = Locator::pkgbuild(text, Flavor::PkgVer);
    assert_eq!(slots.len(), 2);
    assert_eq!(&text[slots[0].span.clone()], "2.0.0");
    assert!(Locator::pkgbuild("pkgrel=1\n", Flavor::PkgVer).is_empty());
  }
}
