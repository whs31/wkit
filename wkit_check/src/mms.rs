mod meta_dependency;
mod meta_value;
mod plugin_meta;
mod version_range;

use wkit_core::{Shell, fs::SourceSet, version::Flavor};
use wkit_manifest::{VersionSection, check::MmsCheck, version::VersionFile};
use wkit_version::{VersionDocument, VersionSet};

pub use self::{
  meta_dependency::MetaDependency, meta_value::MetaValue, plugin_meta::PluginMeta,
  version_range::VersionRange,
};
use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct Mms {
  root: std::path::PathBuf,
  meta: std::path::PathBuf,
  versions: Option<VersionSection>,
}

impl Mms {
  pub const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

  pub fn new(
    root: impl Into<std::path::PathBuf>,
    meta: impl Into<std::path::PathBuf>,
    versions: Option<VersionSection>,
  ) -> Self {
    Self {
      root: root.into(),
      meta: meta.into(),
      versions,
    }
  }

  pub fn plan(
    root: &std::path::Path,
    config: &MmsCheck,
    versions: Option<&VersionSection>,
  ) -> Result<Option<Self>> {
    if config.enabled == Some(false) {
      return Ok(None);
    }
    if let Some(meta) = &config.meta {
      return Ok(Some(Self::new(root, meta, versions.cloned())));
    }
    let found = Self::discover(root)?;
    match (config.enabled, found.as_slice()) {
      (_, [meta]) => Ok(Some(Self::new(root, meta, versions.cloned()))),
      (Some(true), []) => Err(crate::Error::Message(format!(
        "`check.mms` is enabled but there is no `{}`; set `check.mms.meta`",
        MmsCheck::META
      ))),
      (Some(true), _) => Err(crate::Error::Message(format!(
        "`check.mms` found {} `{}` files; set `check.mms.meta` to pick one",
        found.len(),
        MmsCheck::META
      ))),
      _ => Ok(None),
    }
  }

  pub fn discover(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    Ok(
      SourceSet::new(root)
        .extensions(["lua"])
        .exclude(SourceSet::DEFAULT_EXCLUDES.iter().copied())
        .collect()?
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|n| n == MmsCheck::META))
        .map(|p| {
          p.strip_prefix(root)
            .map(std::path::Path::to_owned)
            .unwrap_or(p)
        })
        .collect(),
    )
  }

  pub fn validate(
    &self,
    meta: &PluginMeta,
    project_version: Option<&str>,
  ) -> (Vec<String>, Vec<String>) {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    let path = self.meta.display();
    let uuid = match &meta.uuid {
      None => {
        problems.push(format!("{path}: the plugin has no `uuid`"));
        None
      }
      Some(MetaValue::Literal(uuid)) => {
        problems.extend(Self::uuid_problem(uuid).map(|p| format!("{path}: plugin uuid {p}")));
        Some(uuid.to_ascii_lowercase())
      }
      Some(MetaValue::Dynamic(expr) | MetaValue::Unresolved(expr)) => {
        notes.push(format!("{path}: cannot resolve the plugin uuid `{expr}`"));
        None
      }
    };
    match (&meta.version, project_version) {
      (None, _) => problems.push(format!("{path}: the plugin has no `version`")),
      (Some(MetaValue::Literal(version)), Some(project))
        if !Self::same_version(version, project) =>
      {
        problems.push(format!(
          "{path}: plugin version {version} differs from the project version {project}"
        ));
      }
      (Some(MetaValue::Literal(_)), _) => {}
      (Some(MetaValue::Dynamic(expr)), _) => {
        notes.push(format!("{path}: version follows `{expr}`"));
      }
      (Some(MetaValue::Unresolved(expr)), _) => {
        notes.push(format!(
          "{path}: cannot resolve the plugin version `{expr}`"
        ));
      }
    }
    for dependency in &meta.dependencies {
      let at = format!(
        "{path}:{}: dependency {}",
        dependency.line,
        dependency.label()
      );
      if let Some(MetaValue::Literal(target)) = &dependency.uuid {
        problems.extend(Self::uuid_problem(target).map(|p| format!("{at}: uuid {p}")));
        if uuid.as_deref() == Some(target.to_ascii_lowercase().as_str()) {
          problems.push(format!("{at}: the plugin depends on itself"));
        }
      }
      if let MetaValue::Literal(range) = &dependency.range {
        match VersionRange::parse(range) {
          Ok(parsed) if !parsed.is_satisfiable() => {
            problems.push(format!("{at}: range `{range}` matches no version"));
          }
          Ok(_) => {}
          Err(e) => problems.push(format!("{at}: {e}")),
        }
      }
    }
    (problems, notes)
  }

  fn uuid_problem(uuid: &str) -> Option<String> {
    static UUID: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r"^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$",
      )
      .expect("valid")
    });
    if !UUID.is_match(uuid) {
      Some(format!("`{uuid}` is not a UUID"))
    } else if uuid == Self::NIL_UUID {
      Some("is the nil UUID".to_owned())
    } else {
      None
    }
  }

  fn same_version(a: &str, b: &str) -> bool {
    match (semver::Version::parse(a), semver::Version::parse(b)) {
      (Ok(a), Ok(b)) => (a.major, a.minor, a.patch, a.pre) == (b.major, b.minor, b.patch, b.pre),
      _ => a == b,
    }
  }

  fn project_version(&self) -> Result<Option<String>> {
    if let Some(section) = &self.versions {
      let current = VersionSet::load(&self.root, section)?.current()?;
      return Ok(Some(current.render(Flavor::Full)));
    }
    let conanfile = std::path::Path::new("conanfile.py");
    if !self.root.join(conanfile).is_file() {
      return Ok(None);
    }
    Ok(
      VersionFile::infer(conanfile)
        .and_then(|file| VersionDocument::load(&self.root, &file).ok())
        .map(|document| document.current().to_owned()),
    )
  }
}

impl Check for Mms {
  fn name(&self) -> String {
    "mms".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    shell.status(
      "Checking",
      format_args!("MMS plugin metadata ({})", self.meta.display()),
    )?;
    let text = wkit_core::fs::read_to_string(&self.root.join(&self.meta))?;
    let meta = PluginMeta::parse(&text);
    let project_version = self.project_version()?;
    let (problems, notes) = self.validate(&meta, project_version.as_deref());
    let notes = notes.into_iter().map(|n| format!("  note: {n}"));
    if problems.is_empty() {
      let mut outcome = Outcome::passed(format!(
        "{}{}",
        meta
          .version
          .as_ref()
          .and_then(MetaValue::literal)
          .map_or_else(String::new, |v| format!("v{v}, ")),
        Outcome::plural(meta.dependencies.len(), "dependency")
          .replace("dependencys", "dependencies")
      ));
      outcome.details = notes.collect();
      return Ok(outcome);
    }
    Ok(Outcome::failed(
      Outcome::plural(problems.len(), "problem"),
      problems
        .iter()
        .map(|p| format!("  {p}"))
        .chain(notes)
        .collect(),
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn validate(text: &str, project: Option<&str>) -> Vec<String> {
    let mms = Mms::new("/p", "meta.lua", None);
    mms.validate(&PluginMeta::parse(text), project).0
  }

  #[test]
  fn well_formed_plugins_pass() {
    let text = r#"local uuid = "07245b2b-4453-4a2f-a190-76bfb946c52f"
plugin = {
    uuid = mms.Uuid.from_string(uuid),
    version = mms.Version.parse("1.8.0-rc.2"),
    requires = {
        mms.PluginDependency.new(mms.Uuid.from_string("12dc5ebb-529a-4c62-8340-4db61551d86d"), mms.VersionRange.parse(">=10.2 <11")),
    },
}
"#;
    assert_eq!(
      validate(text, Some("1.8.0-rc.2+build.5")),
      Vec::<String>::new()
    );
    assert_eq!(validate(text, None), Vec::<String>::new());
  }

  #[test]
  fn every_problem_is_reported() {
    let text = r#"local uuid = "07245b2b-4453-4a2f-a190-76bfb946c52f"
plugin = {
    uuid = uuid,
    version = mms.Version.parse("1.0.0"),
    requires = {
        mms.PluginDependency.new(mms.Uuid.from_string(uuid), mms.VersionRange.parse(">=2 <1")),
        mms.PluginDependency.new("not-a-uuid", "latest"),
    },
}
"#;
    let problems = validate(text, Some("1.1.0"));
    let expected = [
      "plugin version 1.0.0 differs from the project version 1.1.0",
      "meta.lua:6: dependency 07245b2b-4453-4a2f-a190-76bfb946c52f: the plugin depends on itself",
      "range `>=2 <1` matches no version",
      "meta.lua:7: dependency not-a-uuid: uuid `not-a-uuid` is not a UUID",
      "`latest` is not a version comparator",
    ];
    for needle in expected {
      assert!(
        problems.iter().any(|p| p.contains(needle)),
        "missing `{needle}` in {problems:#?}"
      );
    }
    assert_eq!(problems.len(), expected.len(), "{problems:#?}");
    let missing = validate(
      "return {\n  uuid = \"00000000-0000-0000-0000-000000000000\",\n}\n",
      None,
    );
    assert_eq!(
      missing,
      [
        "meta.lua: plugin uuid is the nil UUID",
        "meta.lua: the plugin has no `version`"
      ]
    );
  }

  #[test]
  fn a_single_meta_file_turns_the_check_on() {
    let dir = tempfile::tempdir().unwrap();
    let config = MmsCheck::default();
    assert!(Mms::plan(dir.path(), &config, None).unwrap().is_none());
    std::fs::create_dir_all(dir.path().join("src/assets")).unwrap();
    std::fs::write(dir.path().join("src/assets/meta.lua"), "").unwrap();
    let planned = Mms::plan(dir.path(), &config, None).unwrap().unwrap();
    assert_eq!(planned.meta, std::path::Path::new("src/assets/meta.lua"));
    std::fs::create_dir_all(dir.path().join("other")).unwrap();
    std::fs::write(dir.path().join("other/meta.lua"), "").unwrap();
    assert!(Mms::plan(dir.path(), &config, None).unwrap().is_none());
    let forced = MmsCheck {
      enabled: Some(true),
      ..MmsCheck::default()
    };
    let err = Mms::plan(dir.path(), &forced, None).unwrap_err();
    assert!(
      err.to_string().contains("found 2 `meta.lua` files"),
      "{err}"
    );
  }
}
