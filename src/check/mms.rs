use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

use super::Outcome;
use crate::{
  Result, Shell,
  fs::SourceSet,
  manifest::{MmsCheck, VersionFile, VersionSection},
  shell::plural,
  version::{Flavor, VersionDocument, VersionSet},
};

const META: &str = "meta.lua";
const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

#[derive(Debug, Clone)]
pub struct Mms {
  meta: PathBuf,
  versions: Option<VersionSection>,
}

impl Mms {
  pub fn plan(
    root: &Path,
    config: &MmsCheck,
    versions: Option<&VersionSection>,
  ) -> Result<Option<Self>> {
    let mms = |meta: &Path| {
      Some(Self {
        meta: meta.to_owned(),
        versions: versions.cloned(),
      })
    };
    if config.enabled == Some(false) {
      return Ok(None);
    }
    if let Some(meta) = &config.meta {
      return Ok(mms(meta));
    }
    let found: Vec<_> = SourceSet::new(root)
      .extensions(["lua"])
      .collect()?
      .into_iter()
      .filter(|p| p.file_name().is_some_and(|n| n == META))
      .map(|p| p.strip_prefix(root).map(Path::to_owned).unwrap_or(p))
      .collect();
    match (config.enabled, found.as_slice()) {
      (_, [meta]) => Ok(mms(meta)),
      (Some(true), []) => {
        anyhow::bail!("`check.mms` is enabled but there is no `{META}`; set `check.mms.meta`")
      }
      (Some(true), _) => {
        anyhow::bail!(
          "`check.mms` found {} `{META}` files; set `check.mms.meta` to pick one",
          found.len()
        )
      }
      _ => Ok(None),
    }
  }

  fn validate(
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
      Some(Value::Literal(uuid)) => {
        problems.extend(uuid_problem(uuid).map(|p| format!("{path}: plugin uuid {p}")));
        Some(uuid.to_ascii_lowercase())
      }
      Some(Value::Dynamic(expr) | Value::Unresolved(expr)) => {
        notes.push(format!("{path}: cannot resolve the plugin uuid `{expr}`"));
        None
      }
    };
    match (&meta.version, project_version) {
      (None, _) => problems.push(format!("{path}: the plugin has no `version`")),
      (Some(Value::Literal(version)), Some(project)) if !same_version(version, project) => {
        problems.push(format!(
          "{path}: plugin version {version} differs from the project version {project}"
        ));
      }
      (Some(Value::Literal(_)), _) => {}
      (Some(Value::Dynamic(expr)), _) => notes.push(format!("{path}: version follows `{expr}`")),
      (Some(Value::Unresolved(expr)), _) => notes.push(format!(
        "{path}: cannot resolve the plugin version `{expr}`"
      )),
    }
    for dependency in &meta.dependencies {
      let at = format!(
        "{path}:{}: dependency {}",
        dependency.line,
        dependency.label()
      );
      if let Some(Value::Literal(target)) = &dependency.uuid {
        problems.extend(uuid_problem(target).map(|p| format!("{at}: uuid {p}")));
        if uuid.as_deref() == Some(target.to_ascii_lowercase().as_str()) {
          problems.push(format!("{at}: the plugin depends on itself"));
        }
      }
      if let Value::Literal(range) = &dependency.range {
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

  fn project_version(&self, root: &Path) -> Result<Option<String>> {
    if let Some(section) = &self.versions {
      return Ok(Some(
        VersionSet::load(root, section)?
          .current()?
          .render(Flavor::Full),
      ));
    }
    let conanfile = Path::new("conanfile.py");
    if !root.join(conanfile).is_file() {
      return Ok(None);
    }
    Ok(
      VersionFile::infer(conanfile)
        .and_then(|file| VersionDocument::load(root, &file).ok())
        .map(|document| document.current().to_owned()),
    )
  }

  pub fn run(&self, root: &Path, shell: &mut Shell) -> Result<Outcome> {
    shell.status(
      "Checking",
      format_args!("MMS plugin metadata ({})", self.meta.display()),
    )?;
    let meta = PluginMeta::parse(&fs_err::read_to_string(root.join(&self.meta))?);
    let (problems, notes) = self.validate(&meta, self.project_version(root)?.as_deref());
    let notes = notes.into_iter().map(|n| format!("  note: {n}"));
    if problems.is_empty() {
      let version = meta
        .version
        .as_ref()
        .and_then(Value::literal)
        .map_or_else(String::new, |v| format!("v{v}, "));
      let summary = format!("{version}{}", plural(meta.dependencies.len(), "dependency"));
      return Ok(Outcome {
        details: notes.collect(),
        ..Outcome::passed(summary)
      });
    }
    Ok(Outcome::failed(
      plural(problems.len(), "problem"),
      problems
        .iter()
        .map(|p| format!("  {p}"))
        .chain(notes)
        .collect(),
    ))
  }
}

fn uuid_problem(uuid: &str) -> Option<String> {
  if !lazy_regex::regex_is_match!(
    r"^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$",
    uuid
  ) {
    Some(format!("`{uuid}` is not a UUID"))
  } else if uuid == NIL_UUID {
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
  Literal(String),
  Dynamic(String),
  Unresolved(String),
}

impl Value {
  fn literal(&self) -> Option<&str> {
    match self {
      Self::Literal(value) => Some(value),
      Self::Dynamic(_) | Self::Unresolved(_) => None,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Dependency {
  line: usize,
  uuid: Option<Value>,
  name: Option<String>,
  range: Value,
}

impl Dependency {
  fn label(&self) -> String {
    match (&self.name, &self.uuid) {
      (Some(name), _) => format!("`{name}`"),
      (None, Some(Value::Literal(uuid))) => uuid.clone(),
      (None, Some(Value::Dynamic(expr) | Value::Unresolved(expr))) => format!("`{expr}`"),
      (None, None) => "dependency".to_owned(),
    }
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PluginMeta {
  uuid: Option<Value>,
  version: Option<Value>,
  dependencies: Vec<Dependency>,
}

impl PluginMeta {
  fn parse(text: &str) -> Self {
    let local = lazy_regex::regex!(
      r"(?m)^[ \t]*local[ \t]+(?<name>[A-Za-z_]\w*)[ \t]*=[ \t]*(?<expr>.+?)[ \t]*;?[ \t]*$"
    );
    let uuid =
      lazy_regex::regex!(r"(?m)^[ \t]*uuid[ \t]*=[ \t]*(?<expr>.+?)[ \t]*,?[ \t]*(?:--.*)?$");
    let version =
      lazy_regex::regex!(r"(?m)^[ \t]*version[ \t]*=[ \t]*(?<expr>.+?)[ \t]*,?[ \t]*(?:--.*)?$");
    let call = lazy_regex::regex!(
      r#"PluginDependency[.:]new\(\s*(?<uuid>[^,]+?)\s*,\s*(?<range>(?:mms\.)?VersionRange[.:]\w+\([^)]*\)|"[^"]*"|[A-Za-z_][\w.]*)\s*\)"#
    );
    let table = lazy_regex::regex!(r#"\{[^{}]*?\bversion\s*=\s*"(?<range>[^"]*)"[^{}]*\}"#);
    let field = lazy_regex::regex!(r#"\b(?<key>name|uuid)\s*=\s*"(?<value>[^"]*)""#);

    let mut locals = HashMap::new();
    for c in local.captures_iter(text) {
      if let Value::Literal(uuid) = uuid_expr(&c["expr"], &locals) {
        locals.insert(c["name"].to_owned(), uuid);
      }
    }
    let line = |offset: usize| text[..offset].matches('\n').count() + 1;
    let mut dependencies: Vec<_> = call
      .captures_iter(text)
      .map(|c| Dependency {
        line: line(c.get(0).expect("whole match").start()),
        uuid: Some(uuid_expr(&c["uuid"], &locals)),
        name: None,
        range: range_expr(&c["range"]),
      })
      .collect();
    dependencies.extend(table.captures_iter(text).map(|c| {
      let whole = c.get(0).expect("whole match");
      let mut dependency = Dependency {
        line: line(whole.start()),
        uuid: None,
        name: None,
        range: Value::Literal(c["range"].to_owned()),
      };
      for f in field.captures_iter(whole.as_str()) {
        let value = f["value"].to_owned();
        match &f["key"] {
          "name" => dependency.name = Some(value),
          _ => dependency.uuid = Some(Value::Literal(value)),
        }
      }
      dependency
    }));
    dependencies.sort_by_key(|d| d.line);
    Self {
      uuid: uuid.captures(text).map(|c| uuid_expr(&c["expr"], &locals)),
      version: version.captures(text).map(|c| version_expr(&c["expr"])),
      dependencies,
    }
  }
}

fn string(expr: &str) -> Option<&str> {
  expr
    .strip_prefix('"')
    .and_then(|s| s.strip_suffix('"'))
    .filter(|s| !s.contains('"'))
}

fn uuid_expr(expr: &str, locals: &HashMap<String, String>) -> Value {
  if let Some(literal) = string(expr) {
    Value::Literal(literal.to_owned())
  } else if let Some((_, arg)) = lazy_regex::regex_captures!(
    r"^(?:mms\.)?Uuid[.:](?:from_string|new)\(\s*(.+?)\s*\)$",
    expr
  ) {
    uuid_expr(arg, locals)
  } else {
    locals.get(expr).map_or_else(
      || Value::Unresolved(expr.to_owned()),
      |uuid| Value::Literal(uuid.clone()),
    )
  }
}

fn version_expr(expr: &str) -> Value {
  if let Some(literal) = string(expr) {
    Value::Literal(literal.to_owned())
  } else if let Some((_, v)) =
    lazy_regex::regex_captures!(r#"^(?:mms\.)?Version[.:]parse\(\s*"([^"]*)"\s*\)$"#, expr)
  {
    Value::Literal(v.to_owned())
  } else if let Some((_, a, b, c)) = lazy_regex::regex_captures!(
    r"^(?:mms\.)?Version[.:]new\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$",
    expr
  ) {
    Value::Literal(format!("{a}.{b}.{c}"))
  } else if expr == "mms.app.version" {
    Value::Dynamic(expr.to_owned())
  } else {
    Value::Unresolved(expr.to_owned())
  }
}

fn range_expr(expr: &str) -> Value {
  if let Some(literal) = string(expr) {
    Value::Literal(literal.to_owned())
  } else if let Some((_, r)) = lazy_regex::regex_captures!(
    r#"^(?:mms\.)?VersionRange[.:]parse\(\s*"([^"]*)"\s*\)$"#,
    expr
  ) {
    Value::Literal(r.to_owned())
  } else if lazy_regex::regex_is_match!(r"^(?:mms\.)?VersionRange[.:]any\w*\(\s*\)$", expr) {
    Value::Dynamic(expr.to_owned())
  } else {
    Value::Unresolved(expr.to_owned())
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
  version: semver::Version,
  inclusive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct VersionRange {
  lower: Option<Bound>,
  upper: Option<Bound>,
}

impl VersionRange {
  fn parse(text: &str) -> Result<Self, String> {
    let comparator = lazy_regex::regex!(
      r"^(?<op>>=|<=|>|<|=|\^|~)?\s*(?<major>\d+)(?:\.(?<minor>\d+)(?:\.(?<patch>\d+)(?:-(?<pre>[0-9A-Za-z.-]+))?)?)?$"
    );
    let token = lazy_regex::regex!(r"(?:>=|<=|>|<|=|\^|~)?\s*[^\s,<>=^~]+");
    let mut range = Self::default();
    if text.trim() == "*" {
      return Ok(range);
    }
    let tokens: Vec<_> = token.find_iter(text).map(|m| m.as_str()).collect();
    if tokens.is_empty()
      || token
        .replace_all(text, "")
        .chars()
        .any(|c| !c.is_whitespace() && c != ',')
    {
      return Err(format!("`{text}` is not a version range"));
    }
    for token in tokens {
      let c = comparator
        .captures(token)
        .ok_or_else(|| format!("`{token}` is not a version comparator"))?;
      let number = |name: &str| {
        c.name(name)
          .map(|m| m.as_str().parse::<u64>())
          .transpose()
          .map_err(|_| format!("`{token}` is out of range"))
      };
      let depth =
        1 + usize::from(c.name("minor").is_some()) + usize::from(c.name("patch").is_some());
      let mut version = semver::Version::new(
        number("major")?.unwrap_or_default(),
        number("minor")?.unwrap_or_default(),
        number("patch")?.unwrap_or_default(),
      );
      if let Some(pre) = c.name("pre") {
        version.pre =
          semver::Prerelease::new(pre.as_str()).map_err(|e| format!("`{token}`: {e}"))?;
      }
      let partial = depth < 3;
      let next = |depth: usize| match depth {
        1 => semver::Version::new(version.major + 1, 0, 0),
        2 => semver::Version::new(version.major, version.minor + 1, 0),
        _ => semver::Version::new(version.major, version.minor, version.patch + 1),
      };
      let at = |inclusive| Bound {
        version: version.clone(),
        inclusive,
      };
      let below = |depth| Bound {
        version: next(depth),
        inclusive: false,
      };
      match c.name("op").map_or("=", |m| m.as_str()) {
        ">=" => range.raise(at(true)),
        ">" if partial => range.raise(Bound {
          version: next(depth),
          inclusive: true,
        }),
        ">" => range.raise(at(false)),
        "<" => range.lower_to(at(false)),
        "<=" if partial => range.lower_to(below(depth)),
        "<=" => range.lower_to(at(true)),
        "^" => {
          let depth = if version.major > 0 || depth == 1 {
            1
          } else if version.minor > 0 || depth == 2 {
            2
          } else {
            3
          };
          range.raise(at(true));
          range.lower_to(below(depth));
        }
        "~" => {
          range.raise(at(true));
          range.lower_to(below(depth.min(2)));
        }
        _ if partial => {
          range.raise(at(true));
          range.lower_to(below(depth));
        }
        _ => {
          range.raise(at(true));
          range.lower_to(at(true));
        }
      }
    }
    Ok(range)
  }

  fn is_satisfiable(&self) -> bool {
    match (&self.lower, &self.upper) {
      (Some(lower), Some(upper)) => match lower.version.cmp(&upper.version) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Equal => lower.inclusive && upper.inclusive,
        std::cmp::Ordering::Greater => false,
      },
      _ => true,
    }
  }

  fn raise(&mut self, bound: Bound) {
    if self.lower.as_ref().is_none_or(|current| {
      bound.version > current.version || (bound.version == current.version && !bound.inclusive)
    }) {
      self.lower = Some(bound);
    }
  }

  fn lower_to(&mut self, bound: Bound) {
    if self.upper.as_ref().is_none_or(|current| {
      bound.version < current.version || (bound.version == current.version && !bound.inclusive)
    }) {
      self.upper = Some(bound);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn validate(text: &str, project: Option<&str>) -> Vec<String> {
    let mms = Mms {
      meta: META.into(),
      versions: None,
    };
    mms.validate(&PluginMeta::parse(text), project).0
  }

  fn literal(value: &str) -> Value {
    Value::Literal(value.to_owned())
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
    assert_eq!(planned.meta, Path::new("src/assets/meta.lua"));
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

  #[test]
  fn reads_plugins_that_resolve_uuids_through_locals() {
    let meta = PluginMeta::parse(
      r#"local uuid = mms.Uuid.from_string("f04eca57-2fa9-42d3-9528-0dc63e63c3d2")
local planner_uuid = "15cac520-303e-43d6-bce5-5d977134340a"

plugin = {
    uuid = uuid,
    version = mms.Version.parse("3.0.0"),
    requires = {
        mms.PluginDependency.new(mms.Uuid.from_string(planner_uuid), mms.VersionRange.parse(">=1.0 <2")),
        mms.PluginDependency.new(other, mms.VersionRange:any_with_prerelease())
    },
}
"#,
    );
    assert_eq!(
      meta.uuid,
      Some(literal("f04eca57-2fa9-42d3-9528-0dc63e63c3d2"))
    );
    assert_eq!(meta.version, Some(literal("3.0.0")));
    assert_eq!(
      meta.dependencies,
      [
        Dependency {
          line: 8,
          uuid: Some(literal("15cac520-303e-43d6-bce5-5d977134340a")),
          name: None,
          range: literal(">=1.0 <2"),
        },
        Dependency {
          line: 9,
          uuid: Some(Value::Unresolved("other".into())),
          name: None,
          range: Value::Dynamic("mms.VersionRange:any_with_prerelease()".into()),
        },
      ]
    );
  }

  #[test]
  fn reads_returned_tables_with_named_dependencies() {
    let meta = PluginMeta::parse(
      r#"return {
  uuid = "6a1f4b3e-2c9d-4f1a-9e8b-7d2c1a0f5e43",
  version = mms.Version.new(1, 8, 0), -- bumped by wkit
  dependencies = {
    { name = "mms", version = ">=3.12.0" },
  },
}
"#,
    );
    assert_eq!(
      meta.uuid,
      Some(literal("6a1f4b3e-2c9d-4f1a-9e8b-7d2c1a0f5e43"))
    );
    assert_eq!(meta.version, Some(literal("1.8.0")));
    assert_eq!(meta.dependencies[0].name.as_deref(), Some("mms"));
    assert_eq!(meta.dependencies[0].line, 5);
    assert_eq!(
      PluginMeta::parse("plugin = { version = mms.app.version }\n").version,
      None
    );
    assert_eq!(
      PluginMeta::parse("  version = mms.app.version,\n").version,
      Some(Value::Dynamic("mms.app.version".into()))
    );
  }

  fn satisfiable(text: &str) -> bool {
    VersionRange::parse(text).unwrap().is_satisfiable()
  }

  #[test]
  fn ranges_used_by_plugins_are_accepted() {
    for text in [
      ">=10.2 <11",
      ">=3.12.0",
      "^1.4",
      "~2.1.3",
      "1.2",
      "=1.0.0-rc.1",
      "*",
      ">=1.0, <2",
    ] {
      assert!(satisfiable(text), "{text}");
    }
  }

  #[test]
  fn contradictory_ranges_are_empty() {
    for text in [
      ">=11 <10.2",
      ">2 <=2",
      ">=1.5 <1.5",
      "^1.4 >=2",
      ">1.2 <1.3",
    ] {
      assert!(!satisfiable(text), "{text}");
    }
    assert!(satisfiable(">=1.5 <=1.5"));
  }

  #[test]
  fn malformed_ranges_are_rejected() {
    for text in ["", ">=", "=> 1.0", "latest", ">=1.x", "1.2.3.4"] {
      assert!(VersionRange::parse(text).is_err(), "{text}");
    }
  }
}
