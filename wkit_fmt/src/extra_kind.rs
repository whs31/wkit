use wkit_manifest::{Manifest, format::ExtraFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraKind {
  Qmlformat,
  Black,
  Stylua,
}

impl ExtraKind {
  pub const ALL: [Self; 3] = [Self::Qmlformat, Self::Black, Self::Stylua];

  pub fn tool(self) -> &'static str {
    match self {
      Self::Qmlformat => "qmlformat",
      Self::Black => "black",
      Self::Stylua => "stylua",
    }
  }

  pub fn language(self) -> &'static str {
    match self {
      Self::Qmlformat => "QML",
      Self::Black => "Python",
      Self::Stylua => "Lua",
    }
  }

  pub fn default_extensions(self) -> &'static [&'static str] {
    match self {
      Self::Qmlformat => &["qml"],
      Self::Black => &["py", "pyi"],
      Self::Stylua => &["lua"],
    }
  }

  pub fn config(self, manifest: &Manifest) -> &ExtraFormat {
    match self {
      Self::Qmlformat => &manifest.format.qmlformat,
      Self::Black => &manifest.format.black,
      Self::Stylua => &manifest.format.stylua,
    }
  }

  pub fn is_configured(self, root: &std::path::Path) -> bool {
    match self {
      Self::Qmlformat => root.join(".qmlformat.ini").is_file(),
      Self::Black => std::fs::read_to_string(root.join("pyproject.toml"))
        .is_ok_and(|text| text.lines().any(|l| l.trim() == "[tool.black]")),
      Self::Stylua => ["stylua.toml", ".stylua.toml"]
        .iter()
        .any(|name| root.join(name).is_file()),
    }
  }

  pub fn write_flags(self) -> &'static [&'static str] {
    match self {
      Self::Qmlformat => &["-i"],
      Self::Black => &["-q"],
      Self::Stylua => &[],
    }
  }

  pub fn violations(self, root: &std::path::Path, output: &str) -> Vec<std::path::PathBuf> {
    static BLACK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"(?m)^would reformat (?<path>.+?)\r?$").expect("valid")
    });
    static STYLUA: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"(?m)^Diff in (?<path>.+?):\r?$").expect("valid")
    });
    let re = match self {
      Self::Qmlformat => return Vec::new(),
      Self::Black => &BLACK,
      Self::Stylua => &STYLUA,
    };
    re.captures_iter(output)
      .map(|c| {
        let path = std::path::Path::new(&c["path"]);
        path.strip_prefix(root).unwrap_or(path).to_owned()
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn config_files_turn_formatters_on() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!ExtraKind::ALL.iter().any(|k| k.is_configured(dir.path())));
    std::fs::write(dir.path().join(".qmlformat.ini"), "").unwrap();
    std::fs::write(dir.path().join(".stylua.toml"), "").unwrap();
    std::fs::write(dir.path().join("pyproject.toml"), "[tool.ruff]\n").unwrap();
    assert!(ExtraKind::Qmlformat.is_configured(dir.path()));
    assert!(ExtraKind::Stylua.is_configured(dir.path()));
    assert!(!ExtraKind::Black.is_configured(dir.path()));
    std::fs::write(
      dir.path().join("pyproject.toml"),
      "[tool.black]\nline-length = 100\n",
    )
    .unwrap();
    assert!(ExtraKind::Black.is_configured(dir.path()));
  }

  #[test]
  fn check_output_names_the_offending_files() {
    let root = std::path::Path::new("/work/p");
    let black = "would reformat /work/p/scripts/a.py\nwould reformat b.py\nOh no! 💥 💔 💥\n2 files would be reformatted.\n";
    assert_eq!(
      ExtraKind::Black.violations(root, black),
      [std::path::PathBuf::from("scripts/a.py"), "b.py".into()]
    );
    let stylua = "Diff in src/meta.lua:\n1 1    | -local x =  1\n";
    assert_eq!(
      ExtraKind::Stylua.violations(root, stylua),
      [std::path::PathBuf::from("src/meta.lua")]
    );
    assert!(ExtraKind::Qmlformat.violations(root, black).is_empty());
  }
}
