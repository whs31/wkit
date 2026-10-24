use anyhow::Context as _;

use super::{
  Flavor,
  document::{Slot, SlotValue},
};
use crate::{
  Result,
  manifest::{VersionFile, VersionFileKind, VersionPattern},
};

type Value<'i> = toml::Spanned<toml::de::DeValue<'i>>;

#[derive(Debug, Clone, Copy)]
pub enum Locator<'a> {
  Conan,
  CMake,
  Cargo,
  CargoLock { version: &'a str },
  Pkgbuild,
  Patterns(&'a [VersionPattern]),
}

impl<'a> Locator<'a> {
  pub fn of(file: &'a VersionFile) -> Self {
    match file.kind {
      VersionFileKind::Conan => Self::Conan,
      VersionFileKind::CMake => Self::CMake,
      VersionFileKind::Cargo => Self::Cargo,
      VersionFileKind::Pkgbuild => Self::Pkgbuild,
      VersionFileKind::Lua | VersionFileKind::Regex => Self::Patterns(file.patterns()),
    }
  }

  pub fn locate(self, path: &std::path::Path, text: &str, flavor: Flavor) -> Result<Vec<Slot>> {
    let named = |regex: &regex::Regex| {
      regex
        .captures(text)
        .and_then(|c| c.name("version"))
        .map(|m| m.range())
    };
    let span = match self {
      Self::Conan => named(lazy_regex::regex!(
        r#"(?m)^[ \t]*version[ \t]*=[ \t]*["'](?<version>[^"'\n]*)["']"#
      )),
      Self::CMake => named(lazy_regex::regex!(
        r"(?ms)^[ \t]*(?i:project)[ \t]*\([^)]*?\bVERSION\s+(?<version>[^\s)]+)"
      )),
      Self::Cargo => Self::cargo(path, text)?,
      Self::Patterns(patterns) => patterns
        .iter()
        .find_map(|p| p.find(text))
        .map(|m| m.range()),
      Self::CargoLock { version } => return Self::cargo_lock(path, text, version, flavor),
      Self::Pkgbuild => return Ok(Self::pkgbuild(text, flavor)),
    };
    Ok(
      span
        .map(|span| Slot::version(span, flavor))
        .into_iter()
        .collect(),
    )
  }

  fn cargo(path: &std::path::Path, text: &str) -> Result<Option<std::ops::Range<usize>>> {
    let doc = Self::parse_toml(path, text)?;
    let root = doc.get_ref();
    let workspace = root
      .get("workspace")
      .and_then(|w| w.get_ref().get("package"))
      .and_then(|p| p.get_ref().get("version"));
    let package = root.get("package").and_then(|p| p.get_ref().get("version"));
    Ok(
      workspace
        .or(package)
        .and_then(|v| Self::string_span(text, v)),
    )
  }

  fn cargo_lock(
    path: &std::path::Path,
    text: &str,
    version: &str,
    flavor: Flavor,
  ) -> Result<Vec<Slot>> {
    let doc = Self::parse_toml(path, text)?;
    let Some(toml::de::DeValue::Array(packages)) =
      doc.get_ref().get("package").map(toml::Spanned::get_ref)
    else {
      return Ok(Vec::new());
    };
    Ok(
      packages
        .iter()
        .filter(|package| package.get_ref().get("source").is_none())
        .filter_map(|package| package.get_ref().get("version"))
        .filter(|v| v.get_ref().as_str() == Some(version))
        .filter_map(|v| Self::string_span(text, v))
        .map(|span| Slot::version(span, flavor))
        .collect(),
    )
  }

  fn parse_toml<'i>(
    path: &std::path::Path,
    text: &'i str,
  ) -> Result<toml::Spanned<toml::de::DeTable<'i>>> {
    toml::de::DeTable::parse(text).with_context(|| format!("failed to parse `{}`", path.display()))
  }

  fn string_span(text: &str, value: &Value<'_>) -> Option<std::ops::Range<usize>> {
    let inner = value.get_ref().as_str()?;
    let span = value.span();
    let raw = &text[span.clone()];
    let quote = raw.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    (raw.len() == inner.len() + 2 && raw.ends_with(quote)).then(|| span.start + 1..span.end - 1)
  }

  fn pkgbuild(text: &str, flavor: Flavor) -> Vec<Slot> {
    let (mut source, mut pkgver, mut pkgrel) = (None, None, None);
    for captures in
      lazy_regex::regex!(r#"(?m)^(?<key>_source_version|pkgver|pkgrel)=["']?(?<version>[^"'\s#]*)"#)
        .captures_iter(text)
    {
      let value = captures.name("version").expect("group always matches");
      if value.is_empty() || value.as_str().contains('$') {
        continue;
      }
      let target = match &captures["key"] {
        "_source_version" => &mut source,
        "pkgver" => &mut pkgver,
        _ => &mut pkgrel,
      };
      target.get_or_insert(value.range());
    }
    let mut slots: Vec<Slot> = source
      .map(|span| Slot::version(span, Flavor::Full))
      .into_iter()
      .chain(pkgver.map(|span| Slot::version(span, flavor)))
      .collect();
    if !slots.is_empty()
      && let Some(span) = pkgrel
    {
      slots.push(Slot {
        span,
        value: SlotValue::Literal("1"),
      });
    }
    slots
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn find(locator: Locator<'_>, text: &str) -> Option<String> {
    let slots = locator
      .locate(std::path::Path::new("f"), text, Flavor::Full)
      .unwrap();
    slots.first().map(|s| text[s.span.clone()].to_owned())
  }

  #[test]
  fn finds_the_cmake_project_version() {
    let find = |text| find(Locator::CMake, text);
    assert_eq!(
      find("project(mms VERSION 3.12.0 LANGUAGES CXX)").as_deref(),
      Some("3.12.0")
    );
    assert_eq!(
      find("cmake_minimum_required(VERSION 3.25)\n\nproject(rcommon_lua\n  VERSION 1.4.2\n  LANGUAGES CXX\n)\n")
        .as_deref(),
      Some("1.4.2")
    );
    assert_eq!(find("PROJECT(x VERSION 2.0.0)").as_deref(), Some("2.0.0"));
    assert_eq!(
      find("cmake_minimum_required(VERSION 3.25)\nproject(x LANGUAGES C)\n"),
      None
    );
    assert_eq!(find("qt_standard_project_setup(VERSION 6.5)\n"), None);
    assert_eq!(find("# project(x VERSION 1.0.0)\n"), None);
  }

  #[test]
  fn finds_the_conan_class_attribute() {
    let text = "class Pkg(ConanFile):\n    name = \"mms\"\n    version = \"3.12.0\"\n\n    def configure(self):\n        version = str(self.version)\n";
    assert_eq!(find(Locator::Conan, text).as_deref(), Some("3.12.0"));
    assert_eq!(
      find(Locator::Conan, "class Pkg(ConanFile):\n    name = 'x'\n"),
      None
    );
  }

  #[test]
  fn prefers_the_cargo_workspace_package() {
    let text = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.3.1\" # bumped by wkit\n\n[package]\nname = \"root\"\nversion.workspace = true\n";
    assert_eq!(find(Locator::Cargo, text).as_deref(), Some("0.3.1"));
    assert_eq!(
      find(
        Locator::Cargo,
        "[package]\nname = 'x'\nversion = '1.0.0-rc.1'\n"
      )
      .as_deref(),
      Some("1.0.0-rc.1")
    );
  }

  #[test]
  fn skips_inherited_or_escaped_cargo_versions() {
    assert_eq!(
      find(
        Locator::Cargo,
        "[package]\nname = \"x\"\nversion.workspace = true\n"
      ),
      None
    );
    assert_eq!(
      find(Locator::Cargo, "[package]\nversion = \"1.0\\u002E0\"\n"),
      None
    );
    assert!(
      Locator::Cargo
        .locate(std::path::Path::new("Cargo.toml"), "[package", Flavor::Full)
        .is_err()
    );
  }

  #[test]
  fn lock_updates_local_packages_at_the_old_version() {
    let text = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"0.3.1\"\n\n[[package]]\nname = \"serde\"\nversion = \"0.3.1\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"b\"\nversion = \"0.3.1\"\ndependencies = [\"a\"]\n\n[[package]]\nname = \"c\"\nversion = \"2.0.0\"\n";
    let slots = Locator::CargoLock { version: "0.3.1" }
      .locate(std::path::Path::new("Cargo.lock"), text, Flavor::Full)
      .unwrap();
    let names: Vec<_> = slots
      .iter()
      .map(|s| {
        text[..s.span.start]
          .rsplit("name = \"")
          .next()
          .unwrap()
          .split('"')
          .next()
          .unwrap()
      })
      .collect();
    assert_eq!(names, ["a", "b"]);
    assert!(slots.iter().all(|s| &text[s.span.clone()] == "0.3.1"));
  }

  #[test]
  fn finds_source_version_pkgver_and_pkgrel() {
    let text = "pkgname=xlink\n_source_version=1.2.0-rc.1\npkgver=1.2.0rc.1\npkgrel=3\n";
    let found: Vec<_> = Locator::pkgbuild(text, Flavor::PkgVer)
      .into_iter()
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
  fn skips_computed_pkgbuild_values() {
    let text = "_source_version='2.0.0'\npkgver=${_source_version//-/}\npkgrel=1\n";
    let slots = Locator::pkgbuild(text, Flavor::PkgVer);
    assert_eq!(slots.len(), 2);
    assert_eq!(&text[slots[0].span.clone()], "2.0.0");
    assert!(Locator::pkgbuild("pkgrel=1\n", Flavor::PkgVer).is_empty());
  }
}
