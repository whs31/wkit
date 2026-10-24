use wkit_core::version::Flavor;

use super::{VersionFileKind, VersionPattern};
use crate::de::Shorthand;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "Shorthand<VersionFileDetail>")]
pub struct VersionFile {
  pub path: std::path::PathBuf,
  pub kind: VersionFileKind,
  pub flavor: Flavor,
  pub pattern: Option<VersionPattern>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct VersionFileDetail {
  path: std::path::PathBuf,
  kind: Option<VersionFileKind>,
  flavor: Option<Flavor>,
  pattern: Option<String>,
}

impl VersionFile {
  pub fn patterns(&self) -> &[VersionPattern] {
    match (&self.pattern, self.kind) {
      (Some(pattern), _) => std::slice::from_ref(pattern),
      (None, VersionFileKind::Lua) => VersionPattern::lua_defaults(),
      (None, _) => &[],
    }
  }

  pub fn find<'h>(&self, haystack: &'h str) -> Option<regex::Match<'h>> {
    self.patterns().iter().find_map(|p| p.find(haystack))
  }
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
    #[derive(serde::Deserialize)]
    struct Wrap {
      f: VersionFile,
    }
    toml::from_str::<Wrap>(toml)
      .map(|w| w.f)
      .map_err(|e| e.message().to_owned())
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
