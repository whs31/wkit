use super::BuildType;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Bin {
  pub name: String,
  pub path: Option<String>,
  pub package: Option<String>,

  #[serde(default)]
  pub args: Vec<String>,

  #[serde(default)]
  pub env: std::collections::BTreeMap<String, String>,

  pub cwd: Option<std::path::PathBuf>,
}

impl Bin {
  pub fn resolve_path(&self, build_type: BuildType) -> Option<std::path::PathBuf> {
    let template = self.path.as_deref()?;
    Some(
      template
        .replace("{build-type}", &build_type.to_string())
        .replace("{exe}", std::env::consts::EXE_SUFFIX)
        .into(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn path_templates_expand_build_type_and_exe_suffix() {
    let bin = Bin {
      name: "mms".into(),
      path: Some("build/{build-type}/bin/mms{exe}".into()),
      package: None,
      args: vec![],
      env: std::collections::BTreeMap::new(),
      cwd: None,
    };
    let expected = format!(
      "build/RelWithDebInfo/bin/mms{}",
      std::env::consts::EXE_SUFFIX
    );
    assert_eq!(
      bin.resolve_path(BuildType::RelWithDebInfo),
      Some(expected.into())
    );
    assert_eq!(
      Bin { path: None, ..bin }.resolve_path(BuildType::Debug),
      None
    );
  }
}
