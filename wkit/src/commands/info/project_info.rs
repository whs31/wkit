use wkit_build::{BuildOptions, Builder};
use wkit_check::Checker;
use wkit_core::version::Flavor;
use wkit_fmt::Formatter;
use wkit_manifest::Project;
use wkit_version::VersionSet;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectInfo {
  pub name: String,
  pub root: std::path::PathBuf,
  pub version: Option<String>,
  pub languages: Vec<String>,
  pub build: Option<String>,
  pub bins: Vec<String>,
  pub tests: Vec<String>,
  pub formatters: Vec<String>,
  pub checks: Vec<String>,
  pub packages: Vec<String>,
}

impl ProjectInfo {
  pub fn collect(project: &Project) -> anyhow::Result<Self> {
    let m = &project.manifest;
    let version = match &m.version {
      Some(section) => Some(
        VersionSet::load(&project.root, section)?
          .current()?
          .render(Flavor::Full),
      ),
      None => None,
    };
    let formatter = Formatter::new(project)?;
    let mut formatters = Vec::new();
    if formatter.rustfmt.is_some() {
      formatters.push("rustfmt".to_owned());
    }
    if let Some(clang_format) = &formatter.clang_format {
      formatters.push(clang_format.tool().to_string());
    }
    formatters.extend(formatter.extra.iter().map(|f| f.tool().to_string()));
    let mut packages = Vec::new();
    if m.package.deb.is_some() {
      packages.push("deb".to_owned());
    }
    if m.package.arch.locate(&project.root).is_some() {
      packages.push("arch".to_owned());
    }
    Ok(Self {
      name: project.name(),
      root: project.root.clone(),
      version,
      languages: m.languages().iter().map(ToString::to_string).collect(),
      build: Builder::new(project, &BuildOptions::default())
        .system()
        .map(|s| s.to_string()),
      bins: m.bins.iter().map(|b| b.name.clone()).collect(),
      tests: m.test.suites.iter().map(|s| s.runner.to_string()).collect(),
      formatters,
      checks: Checker::plan(project, &[], &[])?.names(),
      packages,
    })
  }

  pub fn fields(&self) -> Vec<(&'static str, String)> {
    let list = |items: &[String]| {
      if items.is_empty() {
        "none".to_owned()
      } else {
        items.join(", ")
      }
    };
    vec![
      ("project", self.name.clone()),
      ("root", self.root.display().to_string()),
      (
        "version",
        self.version.clone().unwrap_or_else(|| "none".to_owned()),
      ),
      ("languages", list(&self.languages)),
      (
        "build",
        self.build.clone().unwrap_or_else(|| "none".to_owned()),
      ),
      ("bins", list(&self.bins)),
      ("tests", list(&self.tests)),
      ("formatters", list(&self.formatters)),
      ("checks", list(&self.checks)),
      ("packages", list(&self.packages)),
    ]
  }
}
