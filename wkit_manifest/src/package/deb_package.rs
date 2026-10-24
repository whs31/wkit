#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DebPackage {
  pub maintainer: Option<String>,
  pub description: Option<String>,
  pub section: Option<String>,
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
  pub prefix: std::path::PathBuf,
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

impl Default for DebPackage {
  fn default() -> Self {
    Self {
      maintainer: None,
      description: None,
      section: None,
      priority: "optional".to_owned(),
      architecture: None,
      homepage: None,
      depends: Vec::new(),
      recommends: Vec::new(),
      suggests: Vec::new(),
      conflicts: Vec::new(),
      breaks: Vec::new(),
      replaces: Vec::new(),
      provides: Vec::new(),
      prefix: "/usr".into(),
      scripts: "packaging/debian".into(),
    }
  }
}
