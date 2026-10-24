use smart_default::SmartDefault;

use super::{BuildType, Task, build::expand, strings};

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PackageSection {
  pub name: Option<String>,
  pub set_option: Option<String>,
  pub output: Option<std::path::PathBuf>,
  pub license: Option<std::path::PathBuf>,
  #[default(strings(PackageSection::DEFAULT_EXCLUDE))]
  pub exclude: Vec<String>,
  pub files: Vec<FileMapping>,
  pub stage: Option<Task>,
  pub deb: Option<DebPackage>,
  pub arch: ArchPackage,
  pub windows: WindowsPackage,
}

impl PackageSection {
  pub const DEFAULT_EXCLUDE: &[&str] = &["include", "lib/cmake", "lib/pkgconfig"];
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DebPackage {
  pub maintainer: Option<String>,
  pub description: Option<String>,
  pub section: Option<String>,
  #[default = "optional"]
  pub priority: String,
  pub architecture: Option<String>,
  pub homepage: Option<String>,
  pub depends: Vec<String>,
  pub recommends: Vec<String>,
  pub suggests: Vec<String>,
  pub conflicts: Vec<String>,
  pub breaks: Vec<String>,
  pub replaces: Vec<String>,
  pub provides: Vec<String>,
  #[default = "/usr"]
  pub prefix: std::path::PathBuf,
  #[default = "packaging/debian"]
  pub scripts: std::path::PathBuf,
}

impl DebPackage {
  pub fn relations(&self) -> [(&'static str, &[String]); 7] {
    [
      ("Depends", &self.depends),
      ("Recommends", &self.recommends),
      ("Suggests", &self.suggests),
      ("Conflicts", &self.conflicts),
      ("Breaks", &self.breaks),
      ("Replaces", &self.replaces),
      ("Provides", &self.provides),
    ]
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ArchPackage {
  pub dir: Option<std::path::PathBuf>,
}

impl ArchPackage {
  pub const DEFAULT_DIRS: &[&str] = &["packaging/arch", "pkg/arch"];

  pub fn locate(&self, root: &std::path::Path) -> Option<std::path::PathBuf> {
    match &self.dir {
      Some(dir) => Some(root.join(dir)),
      None => Self::DEFAULT_DIRS
        .iter()
        .map(|dir| root.join(dir))
        .find(|dir| dir.join("PKGBUILD").is_file()),
    }
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WindowsPackage {
  pub generator: WindowsGenerator,
  pub icon: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
pub enum WindowsGenerator {
  #[default]
  #[display("NSIS64")]
  Nsis,
  #[display("WIX")]
  Wix,
  #[display("ZIP")]
  Zip,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InstallSection {
  pub prefix: Option<std::path::PathBuf>,
  pub files: Vec<FileMapping>,
  pub tmpfiles: Vec<std::path::PathBuf>,
  pub services: Vec<std::path::PathBuf>,
  pub remove: Vec<std::path::PathBuf>,
}

impl InstallSection {
  pub fn is_empty(&self) -> bool {
    self.prefix.is_none()
      && self.files.is_empty()
      && self.tmpfiles.is_empty()
      && self.services.is_empty()
  }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FileMapping {
  pub from: String,
  pub to: String,
  pub mode: Option<FileMode>,
  #[serde(default)]
  pub config: bool,
}

impl FileMapping {
  pub fn is_glob(&self) -> bool {
    self.from.contains(['*', '?', '['])
  }

  pub fn targets_dir(&self) -> bool {
    self.to.ends_with('/')
  }

  pub fn source(&self, build_type: BuildType) -> String {
    expand(&self.from, build_type)
  }

  pub fn destination(&self, source: &std::path::Path) -> std::path::PathBuf {
    if self.targets_dir() {
      std::path::Path::new(&self.to).join(source.file_name().unwrap_or_default())
    } else {
      self.to.clone().into()
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display, serde::Deserialize)]
#[display("{_0:04o}")]
#[serde(try_from = "String")]
pub struct FileMode(pub u32);

impl FileMode {
  #[cfg(unix)]
  pub const EXECUTABLE: Self = Self(0o755);
  pub const REGULAR: Self = Self(0o644);
}

impl TryFrom<String> for FileMode {
  type Error = String;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    u32::from_str_radix(&value, 8)
      .ok()
      .filter(|&mode| mode <= 0o7777)
      .map(Self)
      .ok_or_else(|| format!("`{value}` is not an octal file mode like \"755\""))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_pkgbuild_in_known_dirs() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(ArchPackage::default().locate(dir.path()), None);
    std::fs::create_dir_all(dir.path().join("pkg/arch")).unwrap();
    std::fs::write(dir.path().join("pkg/arch/PKGBUILD"), "").unwrap();
    assert_eq!(
      ArchPackage::default().locate(dir.path()),
      Some(dir.path().join("pkg/arch"))
    );
    let explicit = ArchPackage {
      dir: Some("dist".into()),
    };
    assert_eq!(explicit.locate(dir.path()), Some(dir.path().join("dist")));
  }

  #[test]
  fn directory_targets_keep_the_file_name() {
    let mapping = FileMapping {
      from: "build/{build-type}/relay{exe}".into(),
      to: "/usr/bin/".into(),
      mode: None,
      config: false,
    };
    assert_eq!(
      mapping.source(BuildType::Release),
      format!("build/Release/relay{}", std::env::consts::EXE_SUFFIX)
    );
    assert_eq!(
      mapping.destination(std::path::Path::new("x/relay")),
      std::path::Path::new("/usr/bin/relay")
    );
    assert!(!mapping.is_glob());
  }

  #[test]
  fn file_modes_parse_octal_strings() {
    assert_eq!(FileMode::try_from("0755".to_owned()), Ok(FileMode(0o755)));
    assert_eq!(FileMode(0o644).to_string(), "0644");
    assert!(FileMode::try_from("rwx".to_owned()).is_err());
    assert!(FileMode::try_from("17777".to_owned()).is_err());
  }
}
