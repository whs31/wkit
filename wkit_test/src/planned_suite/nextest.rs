use wkit_manifest::{TestRunner, TestSuite};

use super::PlannedSuite;
use crate::SuiteContext;

impl PlannedSuite {
  pub const NEXTEST_CONFIG: &str = ".config/nextest.toml";
  pub const NEXTEST_PROFILE: &str = "ci";
  pub const NEXTEST_JUNIT: &str = "junit.xml";

  pub(super) fn nextest(ctx: &SuiteContext<'_>, suite: &TestSuite, name: String) -> Self {
    let root = ctx.root();
    let build = &ctx.project.manifest.build.cargo;
    let mut process = build.command(root, &["nextest", "run", "--workspace"]);
    let mut generated = Vec::new();
    let mut junit = None;
    let profile = match (&suite.profile, ctx.junit_file(&name)) {
      (Some(profile), None) => Some(profile.as_str()),
      (None, None) => None,
      (profile, Some(own)) => {
        let profile = profile.as_deref().unwrap_or(Self::NEXTEST_PROFILE);
        let path = Self::nextest_junit_path(root, profile).unwrap_or_else(|| {
          let config = own.with_extension("nextest.toml");
          generated.push((
            config,
            format!(
              "[profile.{profile}.junit]\npath = \"{}\"\n",
              Self::NEXTEST_JUNIT
            ),
          ));
          Self::NEXTEST_JUNIT.to_owned()
        });
        let target = std::env::var_os("CARGO_TARGET_DIR")
          .map_or_else(|| root.join("target"), |dir| root.join(dir));
        junit = Some(target.join("nextest").join(profile).join(path));
        Some(profile)
      }
    };
    for (config, _) in &generated {
      process = process
        .arg("--tool-config-file")
        .arg(format!("wkit:{}", config.display()));
    }
    if let Some(profile) = profile {
      process = process.args(["--profile", profile]);
    }
    if ctx.options.build_type.is_optimized() {
      process = process.arg("--release");
    }
    process = process.args(&suite.args).args(&ctx.options.extra);
    Self {
      name,
      runner: TestRunner::Nextest,
      process,
      junit,
      generated,
    }
  }

  pub fn nextest_junit_path(root: &std::path::Path, profile: &str) -> Option<String> {
    let text = std::fs::read_to_string(root.join(Self::NEXTEST_CONFIG)).ok()?;
    let config: toml::Table = toml::from_str(&text).ok()?;
    config
      .get("profile")?
      .get(profile)?
      .get("junit")?
      .get("path")?
      .as_str()
      .map(ToOwned::to_owned)
  }
}
