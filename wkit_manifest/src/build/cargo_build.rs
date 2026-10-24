#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CargoBuild {
  pub toolchain: Option<String>,
  pub package: Option<String>,
  pub features: Vec<String>,
  pub all_features: bool,
  pub no_default_features: bool,
  pub locked: bool,
  pub args: Vec<String>,
}

impl CargoBuild {
  pub fn command(&self, root: &std::path::Path, subcommand: &[&str]) -> wkit_core::ProcessBuilder {
    let mut cargo = wkit_core::ProcessBuilder::new("cargo").cwd(root);
    if let Some(toolchain) = &self.toolchain {
      cargo = cargo.arg(format!("+{toolchain}"));
    }
    cargo = cargo.args(subcommand);
    if self.all_features {
      cargo = cargo.arg("--all-features");
    } else if !self.features.is_empty() {
      cargo = cargo.arg("--features").arg(self.features.join(","));
    }
    if self.no_default_features {
      cargo = cargo.arg("--no-default-features");
    }
    if self.locked {
      cargo = cargo.arg("--locked");
    }
    cargo
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_carries_the_feature_selection() {
    let build = CargoBuild {
      toolchain: Some("1.88".into()),
      features: vec!["a".into(), "b".into()],
      no_default_features: true,
      locked: true,
      ..CargoBuild::default()
    };
    assert_eq!(
      build
        .command(std::path::Path::new("/r"), &["test", "--workspace"])
        .to_string(),
      "cargo +1.88 test --workspace --features a,b --no-default-features --locked"
    );
  }
}
