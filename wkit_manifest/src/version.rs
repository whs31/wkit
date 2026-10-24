use std::{
  fmt,
  path::{Path, PathBuf},
  sync::LazyLock,
};

use regex::Regex;
use serde::Deserialize;
use wkit_core::version::{Flavor, Scheme};

use crate::de::Shorthand;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VersionSection {
  #[serde(default)]
  pub scheme: Scheme,
  pub source: Option<PathBuf>,
  pub files: Vec<VersionFile>,
}

impl VersionSection {
  pub fn source_file(&self) -> Option<&VersionFile> {
    match &self.source {
      Some(source) => self.files.iter().find(|f| f.path == *source),
      None => self.files.first(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VersionFileKind {
  Conan,
  #[serde(rename = "cmake")]
  CMake,
  Cargo,
  Pkgbuild,
  Lua,
  Regex,
}

impl VersionFileKind {
  pub fn infer(path: &Path) -> Option<Self> {
    match path.file_name()?.to_str()? {
      "conanfile.py" => Some(Self::Conan),
      "CMakeLists.txt" => Some(Self::CMake),
      "Cargo.toml" => Some(Self::Cargo),
      "PKGBUILD" => Some(Self::Pkgbuild),
      name
        if Path::new(name)
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

impl fmt::Display for VersionFileKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Conan => "conan",
      Self::CMake => "cmake",
      Self::Cargo => "cargo",
      Self::Pkgbuild => "pkgbuild",
      Self::Lua => "lua",
      Self::Regex => "regex",
    })
  }
}

#[derive(Debug, Clone)]
pub struct VersionPattern(Regex);

impl PartialEq for VersionPattern {
  fn eq(&self, other: &Self) -> bool {
    self.0.as_str() == other.0.as_str()
  }
}

impl Eq for VersionPattern {}

impl VersionPattern {
  pub fn new(pattern: &str) -> Result<Self, String> {
    let regex = Regex::new(pattern).map_err(|e| format!("invalid pattern: {e}"))?;
    if regex.captures_len() < 2 {
      return Err(format!(
        "pattern `{pattern}` must capture the version, either with a `(?<version>...)` group or a single unnamed group"
      ));
    }
    Ok(Self(regex))
  }

  pub fn as_str(&self) -> &str {
    self.0.as_str()
  }

  pub fn find<'h>(&self, haystack: &'h str) -> Option<regex::Match<'h>> {
    let captures = self.0.captures(haystack)?;
    captures.name("version").or_else(|| captures.get(1))
  }
}

pub static LUA_PATTERNS: LazyLock<[VersionPattern; 2]> = LazyLock::new(|| {
  [
    VersionPattern::new(r#"mms\.Version\.parse\(\s*["'](?<version>[^"']+)["']\s*\)"#)
      .expect("valid"),
    VersionPattern::new(r#"(?m)^\s*version\s*=\s*["'](?<version>[^"']+)["']"#).expect("valid"),
  ]
});

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Shorthand<VersionFileDetail>")]
pub struct VersionFile {
  pub path: PathBuf,
  pub kind: VersionFileKind,
  pub flavor: Flavor,
  pub pattern: Option<VersionPattern>,
}

impl VersionFile {
  pub fn patterns(&self) -> &[VersionPattern] {
    match (&self.pattern, self.kind) {
      (Some(pattern), _) => std::slice::from_ref(pattern),
      (None, VersionFileKind::Lua) => LUA_PATTERNS.as_slice(),
      (None, _) => &[],
    }
  }

  pub fn find<'h>(&self, haystack: &'h str) -> Option<regex::Match<'h>> {
    self.patterns().iter().find_map(|p| p.find(haystack))
  }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct VersionFileDetail {
  path: PathBuf,
  kind: Option<VersionFileKind>,
  flavor: Option<Flavor>,
  pattern: Option<String>,
}

impl TryFrom<Shorthand<VersionFileDetail>> for VersionFile {
  type Error = String;

  fn try_from(value: Shorthand<VersionFileDetail>) -> Result<Self, Self::Error> {
    let detail = match value {
      Shorthand::Str(path) => VersionFileDetail {
        path: path.into(),
        kind: None,
        flavor: None,
        pattern: None,
      },
      Shorthand::Map(detail) => detail,
      Shorthand::Seq(_) => return Err("expected a path or a table with `path`".into()),
    };
    let display = detail.path.display();
    let kind = detail
      .kind
      .or_else(|| detail.pattern.as_ref().map(|_| VersionFileKind::Regex))
      .or_else(|| VersionFileKind::infer(&detail.path))
      .ok_or_else(|| format!("cannot infer how to version `{display}`; set `kind` or `pattern`"))?;
    let pattern = match (&detail.pattern, kind) {
      (Some(p), VersionFileKind::Regex | VersionFileKind::Lua) => Some(VersionPattern::new(p)?),
      (Some(_), kind) => {
        return Err(format!(
          "`pattern` is not supported for {kind} files (`{display}`)"
        ));
      }
      (None, VersionFileKind::Regex) => {
        return Err(format!(
          "`{display}` needs a `pattern` to locate the version"
        ));
      }
      (None, _) => None,
    };
    Ok(Self {
      flavor: detail.flavor.unwrap_or_else(|| kind.default_flavor()),
      path: detail.path,
      kind,
      pattern,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn file(toml: &str) -> Result<VersionFile, String> {
    #[derive(Deserialize)]
    struct Wrap {
      f: VersionFile,
    }
    toml::from_str::<Wrap>(toml)
      .map(|w| w.f)
      .map_err(|e| e.message().to_owned())
  }

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
        "src/plugin/assets/meta.lua",
        VersionFileKind::Lua,
        Flavor::Full,
      ),
    ];
    for (path, kind, flavor) in cases {
      let f = file(&format!("f = {path:?}")).unwrap();
      assert_eq!((f.kind, f.flavor), (kind, flavor), "{path}");
    }
  }

  #[test]
  fn unknown_files_need_a_pattern() {
    let err = file(r#"f = "include/version.h""#).unwrap_err();
    assert!(err.contains("set `kind` or `pattern`"), "{err}");
    let f = file(r#"f = { path = "include/version.h", pattern = '#define GKT_VERSION_STRING "(?<version>[^"]*)"' }"#).unwrap();
    assert_eq!(f.kind, VersionFileKind::Regex);
    let header = "#pragma once\n#define GKT_VERSION_STRING \"1.4.2\"\n";
    assert_eq!(f.find(header).unwrap().as_str(), "1.4.2");
  }

  #[test]
  fn pattern_must_capture() {
    let err = file(r#"f = { path = "v.txt", pattern = "version" }"#).unwrap_err();
    assert!(err.contains("must capture the version"), "{err}");
    let err = file(r#"f = { path = "v.txt", kind = "regex" }"#).unwrap_err();
    assert!(err.contains("needs a `pattern`"), "{err}");
    let err =
      file(r#"f = { path = "conanfile.py", kind = "conan", pattern = "(x)" }"#).unwrap_err();
    assert!(err.contains("not supported for conan"), "{err}");
  }

  #[test]
  fn lua_defaults_match_mms_meta_and_plain_assignments() {
    let lua = file(r#"f = "meta.lua""#).unwrap();
    let meta = "plugin = {\n    version = mms.Version.parse(\"1.0.3\"),\n}";
    assert_eq!(lua.find(meta).unwrap().as_str(), "1.0.3");
    let plain = "return {\n  name = \"x\",\n  version = '2.1.0-rc.1',\n}";
    assert_eq!(lua.find(plain).unwrap().as_str(), "2.1.0-rc.1");
  }

  #[test]
  fn flavor_can_be_overridden() {
    let f = file(r#"f = { path = "PKGBUILD", flavor = "full" }"#).unwrap();
    assert_eq!(f.flavor, Flavor::Full);
  }
}
