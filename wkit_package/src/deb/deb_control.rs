use std::fmt::Write;

use wkit_core::version::{Flavor, ProjectVersion};
use wkit_manifest::package::DebPackage;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebControl {
  pub name: String,
  pub version: String,
  pub architecture: String,
  pub maintainer: String,
  pub installed_size: u64,
  pub description: String,
  pub fields: Vec<(&'static str, String)>,
}

impl DebControl {
  pub fn new(
    name: &str,
    version: &ProjectVersion,
    config: &DebPackage,
    fallback_description: Option<&str>,
  ) -> Result<Self> {
    let maintainer = config.maintainer.clone().ok_or_else(|| {
      Error::setup(
        "Debian packages need a maintainer",
        Some("set `package.deb.maintainer = \"Name <email>\"` in wkit.toml".to_owned()),
      )
    })?;
    let description = config
      .description
      .as_deref()
      .or(fallback_description)
      .filter(|d| !d.trim().is_empty())
      .ok_or_else(|| {
        Error::setup(
          "Debian packages need a description",
          Some("set `project.description` or `package.deb.description`".to_owned()),
        )
      })?;
    let mut fields = Vec::new();
    if let Some(section) = &config.section {
      fields.push(("Section", section.clone()));
    }
    fields.push(("Priority", config.priority.clone()));
    if let Some(homepage) = &config.homepage {
      fields.push(("Homepage", homepage.clone()));
    }
    for (field, values) in config.relations() {
      if !values.is_empty() {
        fields.push((field, values.join(", ")));
      }
    }
    Ok(Self {
      name: name.to_owned(),
      version: version.render(Flavor::Debian),
      architecture: config
        .architecture
        .clone()
        .unwrap_or_else(|| Self::host_architecture().to_owned()),
      maintainer,
      installed_size: 0,
      description: description.trim().to_owned(),
      fields,
    })
  }

  pub fn host_architecture() -> &'static str {
    match std::env::consts::ARCH {
      "x86_64" => "amd64",
      "aarch64" => "arm64",
      "arm" => "armhf",
      "x86" => "i386",
      "powerpc64" => "ppc64el",
      other => other,
    }
  }

  pub fn file_name(&self) -> String {
    format!("{}_{}_{}.deb", self.name, self.version, self.architecture)
  }

  pub fn render(&self) -> String {
    let mut text = format!(
      "Package: {}\nVersion: {}\nArchitecture: {}\nMaintainer: {}\nInstalled-Size: {}\n",
      self.name, self.version, self.architecture, self.maintainer, self.installed_size
    );
    for (field, value) in &self.fields {
      let _ = writeln!(text, "{field}: {value}");
    }
    let mut lines = self.description.lines();
    let _ = writeln!(text, "Description: {}", lines.next().unwrap_or_default());
    for line in lines {
      let line = line.trim_end();
      text.push_str(if line.is_empty() { " .\n" } else { " " });
      if !line.is_empty() {
        text.push_str(line);
        text.push('\n');
      }
    }
    text
  }
}

#[cfg(test)]
mod tests {
  use wkit_core::version::Scheme;

  use super::*;

  #[test]
  fn renders_relations_and_folded_descriptions() {
    let config = DebPackage {
      maintainer: Some("whs31 <whs31@github.io>".into()),
      section: Some("utils".into()),
      architecture: Some("amd64".into()),
      depends: vec!["libc6 (>= 2.28)".into(), "mmsc".into()],
      breaks: vec!["gcs2".into()],
      ..DebPackage::default()
    };
    let version = ProjectVersion::parse("3.2.0-rc.1", Scheme::SemVer).unwrap();
    let mut control = DebControl::new(
      "ics2",
      &version,
      &config,
      Some("Ground control\nRuns the field.\n\nKeeps logs."),
    )
    .unwrap();
    control.installed_size = 42;
    assert_eq!(control.file_name(), "ics2_3.2.0~rc.1_amd64.deb");
    assert_eq!(
      control.render(),
      "Package: ics2\nVersion: 3.2.0~rc.1\nArchitecture: amd64\nMaintainer: whs31 <whs31@github.io>\nInstalled-Size: 42\nSection: utils\nPriority: optional\nDepends: libc6 (>= 2.28), mmsc\nBreaks: gcs2\nDescription: Ground control\n Runs the field.\n .\n Keeps logs.\n"
    );
  }

  #[test]
  fn maintainer_and_description_are_required() {
    let version = ProjectVersion::parse("1.0.0", Scheme::SemVer).unwrap();
    let err = DebControl::new("x", &version, &DebPackage::default(), Some("d")).unwrap_err();
    assert!(err.to_string().contains("package.deb.maintainer"), "{err}");
    let config = DebPackage {
      maintainer: Some("m".into()),
      ..DebPackage::default()
    };
    let err = DebControl::new("x", &version, &config, None).unwrap_err();
    assert!(err.to_string().contains("project.description"), "{err}");
  }
}
