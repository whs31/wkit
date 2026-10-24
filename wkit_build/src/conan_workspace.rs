use crate::{Error, PackageRef, Result, WorkspacePackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConanWorkspace {
  pub packages: Vec<WorkspacePackage>,
}

impl ConanWorkspace {
  pub const YAML: &'static str = "conanws.yml";
  pub const PYTHON: &'static str = "conanws.py";

  pub fn exists(root: &std::path::Path) -> bool {
    root.join(Self::YAML).is_file() || root.join(Self::PYTHON).is_file()
  }

  pub fn load(root: &std::path::Path) -> Result<Option<Self>> {
    let yaml = root.join(Self::YAML);
    if yaml.is_file() {
      let text = wkit_core::fs::read_to_string(&yaml)?;
      let workspace = Self::parse(&text);
      if !workspace.packages.is_empty() || !root.join(Self::PYTHON).is_file() {
        return Ok(Some(workspace));
      }
    }
    if root.join(Self::PYTHON).is_file() {
      return Self::query(root).map(Some);
    }
    Ok(None)
  }

  pub fn parse(text: &str) -> Self {
    let mut packages: Vec<WorkspacePackage> = Vec::new();
    let mut in_packages = false;
    for line in text.lines() {
      let trimmed = line.trim();
      if trimmed.is_empty() || trimmed.starts_with('#') {
        continue;
      }
      if !line.starts_with([' ', '\t', '-']) {
        in_packages = trimmed == "packages:";
        continue;
      }
      if !in_packages {
        continue;
      }
      let (item, field) = match trimmed.strip_prefix("- ") {
        Some(field) => (true, field),
        None => (false, trimmed),
      };
      if item {
        packages.push(WorkspacePackage {
          path: std::path::PathBuf::new(),
          reference: None,
        });
      }
      let (Some(package), Some((key, value))) = (packages.last_mut(), field.split_once(':')) else {
        continue;
      };
      let value = value.trim().trim_matches(['"', '\'']);
      match key.trim() {
        "path" => package.path = value.into(),
        "ref" => package.reference = Some(PackageRef::parse(value)),
        _ => {}
      }
    }
    packages.retain(|p| !p.path.as_os_str().is_empty());
    Self { packages }
  }

  fn query(root: &std::path::Path) -> Result<Self> {
    let output = wkit_core::ProcessBuilder::new("conan")
      .cwd(root)
      .args(["workspace", "info", "--format", "json"])
      .output()?;
    let info: serde_json::Value = serde_json::from_slice(&output.stdout)
      .map_err(|e| Error::Message(format!("could not read the workspace: {e}")))?;
    let packages = info["packages"]
      .as_array()
      .into_iter()
      .flatten()
      .filter_map(|package| {
        Some(WorkspacePackage {
          path: package["path"].as_str()?.into(),
          reference: package["ref"].as_str().map(PackageRef::parse),
        })
      })
      .collect();
    Ok(Self { packages })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_packages_from_conanws_yml() {
    let workspace = ConanWorkspace::parse(
      "# radar\npackages:\n  - path: core\n    ref: radar-core/0.5.0\n  - ref: \"radar-app/0.5.0\"\n    path: 'app'\n  - path: tools\nother:\n  - path: ignored\n",
    );
    let found: Vec<_> = workspace
      .packages
      .iter()
      .map(|p| {
        format!(
          "{} {}",
          p.path.display(),
          p.reference
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
        )
      })
      .collect();
    assert_eq!(
      found,
      ["core radar-core v0.5.0", "app radar-app v0.5.0", "tools "]
    );
  }
}
