use wkit_core::version::Flavor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum VersionFileKind {
  Conan,
  CMake,
  Cargo,
  Pkgbuild,
  Lua,
  Regex,
}

impl VersionFileKind {
  pub fn infer(path: &std::path::Path) -> Option<Self> {
    match path.file_name()?.to_str()? {
      "conanfile.py" => Some(Self::Conan),
      "CMakeLists.txt" => Some(Self::CMake),
      "Cargo.toml" => Some(Self::Cargo),
      "PKGBUILD" => Some(Self::Pkgbuild),
      name
        if std::path::Path::new(name)
          .extension()
          .is_some_and(|e| e.eq_ignore_ascii_case("lua")) =>
      {
        Some(Self::Lua)
      }
      _ => None,
    }
  }

  pub fn default_flavor(self) -> Flavor {
    match self {
      Self::CMake => Flavor::Numeric,
      Self::Pkgbuild => Flavor::PkgVer,
      Self::Conan | Self::Cargo | Self::Lua | Self::Regex => Flavor::Full,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn kinds_and_flavors_are_inferred_from_file_names() {
    let cases = [
      ("conanfile.py", VersionFileKind::Conan, Flavor::Full),
      ("CMakeLists.txt", VersionFileKind::CMake, Flavor::Numeric),
      ("crates/x/Cargo.toml", VersionFileKind::Cargo, Flavor::Full),
      (
        "packaging/arch/PKGBUILD",
        VersionFileKind::Pkgbuild,
        Flavor::PkgVer,
      ),
      (
        "src/plugin/assets/meta.LUA",
        VersionFileKind::Lua,
        Flavor::Full,
      ),
    ];
    for (path, kind, flavor) in cases {
      let inferred = VersionFileKind::infer(std::path::Path::new(path)).unwrap();
      assert_eq!(
        (inferred, inferred.default_flavor()),
        (kind, flavor),
        "{path}"
      );
    }
    assert_eq!(
      VersionFileKind::infer(std::path::Path::new("version.h")),
      None
    );
  }

  #[test]
  fn displays_like_manifest_values() {
    assert_eq!(VersionFileKind::CMake.to_string(), "cmake");
    assert_eq!(VersionFileKind::Pkgbuild.to_string(), "pkgbuild");
  }
}
