use std::collections::BTreeSet;

use serde::Deserialize;
use wkit_core::Language;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectSection {
  pub name: Option<String>,
  pub description: Option<String>,
  pub languages: BTreeSet<Language>,
  pub default_run: Option<String>,
  #[serde(default, deserialize_with = "crate::de::version_req")]
  pub wkit: Option<semver::VersionReq>,
}
