use wkit_core::{Shell, shell::Elapsed};
use wkit_fmt::Formatter;
use wkit_manifest::Project;

use crate::{
  Check, CheckOptions, ClangTidy, Clippy, Custom, Error, FormatCheck, Mms, Outcome, PreCommitPins,
  Qmllint, Result, Secrets, StageReport, VersionsCheck, Whitespace,
};

#[derive(Debug)]
pub struct Checker {
  checks: Vec<Box<dyn Check>>,
}

impl Checker {
  pub fn plan(project: &Project, only: &[String], skip: &[String]) -> Result<Self> {
    let m = &project.manifest;
    let root = &project.root;
    let formatter = Formatter::new(project)?;
    let mut planned: Vec<(String, Option<Box<dyn Check>>)> = vec![
      (
        "format".into(),
        (m.check.format && !formatter.is_empty())
          .then(|| Box::new(FormatCheck::new(formatter)) as Box<dyn Check>),
      ),
      (
        "versions".into(),
        m.version
          .clone()
          .filter(|_| m.check.versions)
          .map(|section| Box::new(VersionsCheck::new(root, section)) as Box<dyn Check>),
      ),
      (
        "clippy".into(),
        m.clippy_enabled()
          .then(|| Box::new(Clippy::new(root, m.check.clippy.clone())) as Box<dyn Check>),
      ),
      (
        "clang-tidy".into(),
        m.clang_tidy_enabled().then(|| {
          let tool = project.tool(ClangTidy::TOOL);
          Box::new(ClangTidy::new(root, tool, m.check.clang_tidy.clone())) as Box<dyn Check>
        }),
      ),
      (
        "secrets".into(),
        m.check
          .secrets
          .then(|| Box::new(Secrets::new(root)) as Box<dyn Check>),
      ),
      (
        "whitespace".into(),
        m.check
          .whitespace
          .then(|| Box::new(Whitespace::new(root)) as Box<dyn Check>),
      ),
      (
        "pre-commit".into(),
        m.check
          .pre_commit
          .then(|| Box::new(PreCommitPins::new(root, m.tools.clone())) as Box<dyn Check>),
      ),
      (
        "qmllint".into(),
        m.check.qmllint.enabled.then(|| {
          let tool = project.tool(Qmllint::TOOL);
          Box::new(Qmllint::new(root, tool, m.check.qmllint.clone())) as Box<dyn Check>
        }),
      ),
      (
        "mms".into(),
        Mms::plan(root, &m.check.mms, m.version.as_ref())?
          .map(|mms| Box::new(mms) as Box<dyn Check>),
      ),
    ];
    for custom in &m.check.custom {
      planned.push((
        custom.name.clone(),
        Some(Box::new(Custom::new(root, custom.clone()))),
      ));
    }

    let known: Vec<_> = planned.iter().map(|(name, _)| name.clone()).collect();
    for name in only.iter().chain(skip) {
      if !known.contains(name) {
        return Err(Error::Message(format!(
          "unknown check `{name}`; expected one of: {}",
          known.join(", ")
        )));
      }
    }
    let mut checks = Vec::new();
    for (name, check) in planned {
      if skip.contains(&name) || (!only.is_empty() && !only.contains(&name)) {
        continue;
      }
      match check {
        Some(check) => checks.push(check),
        None if only.contains(&name) => {
          return Err(Error::Message(format!(
            "check `{name}` is disabled for this project"
          )));
        }
        None => {}
      }
    }
    Ok(Self { checks })
  }

  pub fn names(&self) -> Vec<String> {
    self.checks.iter().map(|c| c.name()).collect()
  }

  pub fn run(&self, shell: &mut Shell, options: &CheckOptions) -> Result<Vec<StageReport>> {
    let mut reports = Vec::with_capacity(self.checks.len());
    for check in &self.checks {
      let start = std::time::Instant::now();
      let outcome = check.run(shell, options).unwrap_or_else(|e| {
        let mut details = Vec::new();
        let mut source = std::error::Error::source(&e);
        while let Some(cause) = source {
          details.extend(cause.to_string().lines().map(|l| format!("  {l}")));
          source = cause.source();
        }
        Outcome::failed(e.to_string(), details)
      });
      for line in &outcome.details {
        writeln!(shell.err(), "{line}")?;
      }
      reports.push(StageReport {
        name: check.name(),
        outcome,
        elapsed: start.elapsed(),
      });
    }
    Self::summarize(shell, &reports)?;
    Ok(reports)
  }

  fn summarize(shell: &mut Shell, reports: &[StageReport]) -> Result<()> {
    let name_width = reports
      .iter()
      .map(|r| r.name.len())
      .max()
      .unwrap_or_default();
    let summary_width = reports
      .iter()
      .map(|r| r.outcome.summary.lines().next().unwrap_or_default().len())
      .max()
      .unwrap_or_default();
    for report in reports {
      let status = report.outcome.status;
      shell.status_with_style(
        status,
        format_args!(
          "{:<name_width$}  {:<summary_width$}  {}",
          report.name,
          report.outcome.summary.lines().next().unwrap_or_default(),
          Elapsed(report.elapsed)
        ),
        status.style(),
      )?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use wkit_manifest::Manifest;

  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn plans_stages_from_the_manifest() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"rust\"]\n\n[[check.custom]]\nname = \"meta\"\nrun = \"true\"\n",
    );
    let checker = Checker::plan(&project, &[], &[]).unwrap();
    assert_eq!(
      checker.names(),
      [
        "format",
        "clippy",
        "secrets",
        "whitespace",
        "pre-commit",
        "meta"
      ]
    );
    let only = Checker::plan(&project, &["clippy".into(), "meta".into()], &[]).unwrap();
    assert_eq!(only.names(), ["clippy", "meta"]);
    let skip = Checker::plan(&project, &[], &["format".into(), "whitespace".into()]).unwrap();
    assert_eq!(skip.names(), ["clippy", "secrets", "pre-commit", "meta"]);
  }

  #[test]
  fn rejects_unknown_and_disabled_stages() {
    let (_dir, project) = project("[project]\nlanguages = [\"rust\"]\n");
    let unknown = Checker::plan(&project, &["clipy".into()], &[]).unwrap_err();
    assert!(unknown.to_string().contains("unknown check `clipy`"));
    let disabled = Checker::plan(&project, &["clang-tidy".into()], &[]).unwrap_err();
    assert!(disabled.to_string().contains("disabled"));
    let _ = Manifest::parse("[project]\nlanguages = [\"c\"]\n").unwrap();
  }

  #[test]
  fn failures_and_errors_are_reported_per_stage() {
    let (dir, project) = project(
      "[project]\nlanguages = [\"c\"]\n\n[version]\nfiles = [\"CMakeLists.txt\"]\n\n[[check.custom]]\nname = \"fails\"\nrun = [\"false\"]\n",
    );
    std::fs::write(
      dir.path().join("CMakeLists.txt"),
      "project(x VERSION 1.0.0)\n",
    )
    .unwrap();
    let checker = Checker::plan(
      &project,
      &["versions".into(), "clang-tidy".into(), "fails".into()],
      &[],
    )
    .unwrap();
    let mut shell = Shell::from_write(Box::new(std::io::sink()));
    let reports = checker.run(&mut shell, &CheckOptions::default()).unwrap();
    let failed: Vec<_> = reports
      .iter()
      .map(|r| (r.name.as_str(), r.failed()))
      .collect();
    assert_eq!(
      failed,
      [("versions", false), ("clang-tidy", true), ("fails", true)]
    );
    assert!(reports[1].outcome.summary.contains("compile_commands.json"));
  }
}
