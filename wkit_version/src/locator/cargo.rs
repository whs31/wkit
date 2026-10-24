use wkit_core::version::Flavor;

use super::Locator;
use crate::{Result, Slot};

type Value<'i> = toml::Spanned<toml::de::DeValue<'i>>;

impl Locator<'_> {
  pub(super) fn cargo(path: &std::path::Path, text: &str, flavor: Flavor) -> Result<Option<Slot>> {
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
        .and_then(|v| Self::string_span(text, v))
        .map(|span| Slot::version(span, flavor)),
    )
  }

  pub(super) fn cargo_lock(
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
    toml::de::DeTable::parse(text).map_err(|source| {
      wkit_core::Error::Toml {
        path: path.to_owned(),
        source: Box::new(source),
      }
      .into()
    })
  }

  fn string_span(text: &str, value: &Value<'_>) -> Option<std::ops::Range<usize>> {
    let inner = value.get_ref().as_str()?;
    let span = value.span();
    let raw = &text[span.clone()];
    let quote = raw.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    (raw.len() == inner.len() + 2 && raw.ends_with(quote)).then(|| span.start + 1..span.end - 1)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn find(text: &str) -> Option<&str> {
    Locator::cargo(std::path::Path::new("Cargo.toml"), text, Flavor::Full)
      .unwrap()
      .map(|s| &text[s.span])
  }

  #[test]
  fn prefers_the_workspace_package() {
    let text = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.3.1\" # bumped by wkit\n\n[package]\nname = \"root\"\nversion.workspace = true\n";
    assert_eq!(find(text), Some("0.3.1"));
    assert_eq!(
      find("[package]\nname = 'x'\nversion = '1.0.0-rc.1'\n"),
      Some("1.0.0-rc.1")
    );
  }

  #[test]
  fn skips_inherited_or_escaped_versions() {
    assert_eq!(
      find("[package]\nname = \"x\"\nversion.workspace = true\n"),
      None
    );
    assert_eq!(find("[package]\nversion = \"1.0\\u002E0\"\n"), None);
    assert!(Locator::cargo(std::path::Path::new("Cargo.toml"), "[package", Flavor::Full).is_err());
  }

  #[test]
  fn lock_updates_local_packages_at_the_old_version() {
    let text = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"0.3.1\"\n\n[[package]]\nname = \"serde\"\nversion = \"0.3.1\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"b\"\nversion = \"0.3.1\"\ndependencies = [\"a\"]\n\n[[package]]\nname = \"c\"\nversion = \"2.0.0\"\n";
    let slots = Locator::cargo_lock(
      std::path::Path::new("Cargo.lock"),
      text,
      "0.3.1",
      Flavor::Full,
    )
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
}
