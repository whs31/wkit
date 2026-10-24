mod cargo;
mod cmake;
mod conan;
mod pkgbuild;

use wkit_core::version::Flavor;
use wkit_manifest::{VersionFile, VersionFileKind, version::VersionPattern};

use crate::{Result, Slot};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Locator<'a> {
  Conan,
  CMake,
  Cargo,
  CargoLock { version: &'a str },
  Pkgbuild,
  Patterns(&'a [VersionPattern]),
}

impl<'a> Locator<'a> {
  pub(crate) fn of(file: &'a VersionFile) -> Self {
    match file.kind {
      VersionFileKind::Conan => Self::Conan,
      VersionFileKind::CMake => Self::CMake,
      VersionFileKind::Cargo => Self::Cargo,
      VersionFileKind::Pkgbuild => Self::Pkgbuild,
      VersionFileKind::Lua | VersionFileKind::Regex => Self::Patterns(file.patterns()),
    }
  }

  pub(crate) fn locate(
    self,
    path: &std::path::Path,
    text: &str,
    flavor: Flavor,
  ) -> Result<Vec<Slot>> {
    Ok(match self {
      Self::Conan => Self::conan(text, flavor).into_iter().collect(),
      Self::CMake => Self::cmake(text, flavor).into_iter().collect(),
      Self::Cargo => Self::cargo(path, text, flavor)?.into_iter().collect(),
      Self::CargoLock { version } => Self::cargo_lock(path, text, version, flavor)?,
      Self::Pkgbuild => Self::pkgbuild(text, flavor),
      Self::Patterns(patterns) => patterns
        .iter()
        .find_map(|p| p.find(text))
        .map(|m| Slot::version(m.range(), flavor))
        .into_iter()
        .collect(),
    })
  }

  fn first_match(regex: &regex::Regex, text: &str) -> Option<std::ops::Range<usize>> {
    regex
      .captures(text)
      .and_then(|c| c.name("version"))
      .map(|m| m.range())
  }
}
