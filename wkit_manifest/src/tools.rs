mod tool_entry;

use wkit_core::tool::{Tool, ToolSource};

pub use self::tool_entry::ToolEntry;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(transparent)]
pub struct Tools(std::collections::BTreeMap<String, ToolEntry>);

impl Tools {
  pub const DEFAULTS: &[(&str, &str)] = &[("clang-format", "23.1.3"), ("clang-tidy", "22.1.8")];
  pub const QT_TOOLS: &[&str] = &["qmlformat", "qmllint", "qmlls"];
  pub const QT_BIN_DIRS: &[&str] = &[
    "/usr/lib/qt6/bin",
    "/usr/lib64/qt6/bin",
    "/usr/lib/x86_64-linux-gnu/qt6/bin",
    "/usr/lib/aarch64-linux-gnu/qt6/bin",
  ];

  pub fn get(&self, name: &str) -> Option<&ToolEntry> {
    self.0.get(name)
  }

  pub fn iter(&self) -> impl Iterator<Item = (&str, &ToolEntry)> {
    self.0.iter().map(|(k, v)| (k.as_str(), v))
  }

  pub fn resolve(&self, name: &str, root: &std::path::Path) -> Tool {
    let source = match self.0.get(name) {
      Some(ToolEntry::Pinned { package, version }) => ToolSource::Uv {
        package: package.clone().unwrap_or_else(|| name.to_owned()),
        version: version.clone(),
      },
      Some(ToolEntry::Path(path)) => ToolSource::Path(root.join(path)),
      Some(ToolEntry::System) => ToolSource::System,
      None => match Self::DEFAULTS.iter().find(|(tool, _)| *tool == name) {
        Some((_, version)) => ToolSource::Uv {
          package: name.to_owned(),
          version: (*version).to_owned(),
        },
        None => {
          Self::qt_binary(name, &Self::qt_bin_dirs()).map_or(ToolSource::System, ToolSource::Path)
        }
      },
    };
    Tool {
      name: name.to_owned(),
      source,
    }
  }

  pub fn qt_binary(name: &str, dirs: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    if !Self::QT_TOOLS.contains(&name) {
      return None;
    }
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    dirs
      .iter()
      .map(|dir| dir.join(&exe))
      .find(|path| path.is_file())
  }

  fn qt_bin_dirs() -> Vec<std::path::PathBuf> {
    std::env::var_os("QT_HOST_BINS")
      .map(std::path::PathBuf::from)
      .into_iter()
      .chain(Self::QT_BIN_DIRS.iter().map(std::path::PathBuf::from))
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use super::*;

  fn tools(toml: &str) -> Tools {
    toml::from_str(toml).unwrap()
  }

  #[test]
  fn defaults_are_used_when_not_pinned() {
    let t = tools("").resolve("clang-tidy", Path::new("/p"));
    assert_eq!(t, Tool::pinned("clang-tidy", "22.1.8"));
    assert_eq!(
      tools("").resolve("cmake", Path::new("/p")),
      Tool::system("cmake")
    );
  }

  #[test]
  fn qt_tools_prefer_qt6_installations() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = [dir.path().join("missing"), dir.path().to_owned()];
    assert_eq!(Tools::qt_binary("qmllint", &dirs), None);
    let qmllint = dir
      .path()
      .join(format!("qmllint{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&qmllint, "").unwrap();
    assert_eq!(Tools::qt_binary("qmllint", &dirs), Some(qmllint));
    assert_eq!(Tools::qt_binary("cmake", &dirs), None);
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
