use crate::de::Shorthand;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "Shorthand<ToolDetail>")]
pub enum ToolEntry {
  Pinned {
    package: Option<String>,
    version: String,
  },
  Path(std::path::PathBuf),
  System,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ToolDetail {
  version: Option<String>,
  package: Option<String>,
  path: Option<std::path::PathBuf>,
}

impl ToolEntry {
  pub const SYSTEM: &str = "system";
}

impl TryFrom<Shorthand<ToolDetail>> for ToolEntry {
  type Error = &'static str;

  fn try_from(value: Shorthand<ToolDetail>) -> Result<Self, Self::Error> {
    match value {
      Shorthand::Str(v) if v == Self::SYSTEM => Ok(Self::System),
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
      }) if v == Self::SYSTEM => Ok(Self::System),
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
