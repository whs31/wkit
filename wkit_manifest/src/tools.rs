use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};

use serde::Deserialize;
use wkit_core::tool::{Tool, ToolSource};

use crate::de::Shorthand;

pub const DEFAULT_TOOLS: &[(&str, &str)] = &[("clang-format", "23.1.3"), ("clang-tidy", "22.1.8")];

const SYSTEM: &str = "system";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Tools(BTreeMap<String, ToolEntry>);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Shorthand<ToolDetail>")]
pub enum ToolEntry {
  Pinned {
    package: Option<String>,
    version: String,
  },
  Path(PathBuf),
  System,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ToolDetail {
  version: Option<String>,
  package: Option<String>,
  path: Option<PathBuf>,
}

impl TryFrom<Shorthand<ToolDetail>> for ToolEntry {
  type Error = &'static str;

  fn try_from(value: Shorthand<ToolDetail>) -> Result<Self, Self::Error> {
    match value {
      Shorthand::Str(v) if v == SYSTEM => Ok(Self::System),
      Shorthand::Str(version) => Ok(Self::Pinned {
        package: None,
        version,
      }),
      Shorthand::Map(ToolDetail {
        path: Some(path),
        version: None,
        package: None,
      }) => Ok(Self::Path(path)),
      Shorthand::Map(ToolDetail { path: Some(_), .. }) => {
        Err("`path` cannot be combined with `version` or `package`")
      }
      Shorthand::Map(ToolDetail {
        version: Some(v), ..
      }) if v == SYSTEM => Ok(Self::System),
      Shorthand::Map(ToolDetail {
        version: Some(version),
        package,
        ..
      }) => Ok(Self::Pinned { package, version }),
      Shorthand::Map(_) => Err("a tool needs either `version` or `path`"),
      Shorthand::Seq(_) => Err("expected a version string or a table"),
    }
  }
}

impl Tools {
  pub fn get(&self, name: &str) -> Option<&ToolEntry> {
    self.0.get(name)
  }

  pub fn iter(&self) -> impl Iterator<Item = (&str, &ToolEntry)> {
    self.0.iter().map(|(k, v)| (k.as_str(), v))
  }

  pub fn resolve(&self, name: &str, root: &Path) -> Tool {
    let source = match self.0.get(name) {
      Some(ToolEntry::Pinned { package, version }) => ToolSource::Uv {
        package: package.clone().unwrap_or_else(|| name.to_owned()),
        version: version.clone(),
      },
      Some(ToolEntry::Path(path)) => ToolSource::Path(root.join(path)),
      Some(ToolEntry::System) => ToolSource::System,
      None => match DEFAULT_TOOLS.iter().find(|(tool, _)| *tool == name) {
        Some((_, version)) => ToolSource::Uv {
          package: name.to_owned(),
          version: (*version).to_owned(),
        },
        None => ToolSource::System,
      },
    };
    Tool {
      name: name.to_owned(),
      source,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tools(toml: &str) -> Tools {
    toml::from_str(toml).unwrap()
  }

  #[test]
  fn defaults_are_used_when_not_pinned() {
    let t = tools("").resolve("clang-tidy", Path::new("/p"));
    assert_eq!(t, Tool::pinned("clang-tidy", "22.1.8"));
    assert_eq!(
      tools("").resolve("qmllint", Path::new("/p")),
      Tool::system("qmllint")
    );
  }

  #[test]
  fn project_pins_override_defaults() {
    let t = tools(
      r#"
      clang-format = "19.1.0"
      clang-tidy = { path = "tools/clang-tidy" }
      black = { version = "25.9.0", package = "black" }
      qmlformat = "system"
      "#,
    );
    let root = Path::new("/p");
    assert_eq!(
      t.resolve("clang-format", root),
      Tool::pinned("clang-format", "19.1.0")
    );
    assert_eq!(
      t.resolve("clang-tidy", root).source,
      ToolSource::Path("/p/tools/clang-tidy".into())
    );
    assert_eq!(t.resolve("qmlformat", root).source, ToolSource::System);
    assert_eq!(t.resolve("black", root), Tool::pinned("black", "25.9.0"));
  }

  #[test]
  fn conflicting_entries_are_rejected() {
    let err = toml::from_str::<Tools>(r#"x = { path = "a", version = "1" }"#).unwrap_err();
    assert!(err.message().contains("cannot be combined"), "{err}");
    let err = toml::from_str::<Tools>("x = {}").unwrap_err();
    assert!(
      err.message().contains("either `version` or `path`"),
      "{err}"
    );
  }
}
