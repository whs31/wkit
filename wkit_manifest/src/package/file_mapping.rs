use super::FileMode;

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

  pub fn source(&self, build_type: crate::BuildType) -> String {
    self
      .from
      .replace("{build-type}", &build_type.to_string())
      .replace("{exe}", std::env::consts::EXE_SUFFIX)
  }

  pub fn destination(&self, source: &std::path::Path) -> std::path::PathBuf {
    if self.targets_dir() {
      std::path::Path::new(&self.to).join(source.file_name().unwrap_or_default())
    } else {
      self.to.clone().into()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn directory_targets_keep_the_file_name() {
    let mapping = FileMapping {
      from: "build/{build-type}/relay{exe}".into(),
      to: "/usr/bin/".into(),
      mode: None,
      config: false,
    };
    let source = mapping.source(crate::BuildType::Release);
    assert_eq!(
      source,
      format!("build/Release/relay{}", std::env::consts::EXE_SUFFIX)
    );
    assert_eq!(
      mapping.destination(std::path::Path::new("x/relay")),
      std::path::Path::new("/usr/bin/relay")
    );
    assert!(!mapping.is_glob());
  }
}
