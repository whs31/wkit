use crate::mms::{MetaDependency, MetaValue};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginMeta {
  pub uuid: Option<MetaValue>,
  pub version: Option<MetaValue>,
  pub dependencies: Vec<MetaDependency>,
}

impl PluginMeta {
  pub fn parse(text: &str) -> Self {
    static LOCAL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r"(?m)^[ \t]*local[ \t]+(?<name>[A-Za-z_]\w*)[ \t]*=[ \t]*(?<expr>.+?)[ \t]*;?[ \t]*$",
      )
      .expect("valid")
    });
    static UUID: std::sync::LazyLock<regex::Regex> =
      std::sync::LazyLock::new(|| PluginMeta::key("uuid"));
    static VERSION: std::sync::LazyLock<regex::Regex> =
      std::sync::LazyLock::new(|| PluginMeta::key("version"));
    static CALL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r#"PluginDependency[.:]new\(\s*(?<uuid>[^,]+?)\s*,\s*(?<range>(?:mms\.)?VersionRange[.:]\w+\([^)]*\)|"[^"]*"|[A-Za-z_][\w.]*)\s*\)"#,
      )
      .expect("valid")
    });
    static TABLE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r#"\{[^{}]*?\bversion\s*=\s*"(?<range>[^"]*)"[^{}]*\}"#).expect("valid")
    });
    static FIELD: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r#"\b(?<key>name|uuid)\s*=\s*"(?<value>[^"]*)""#).expect("valid")
    });

    let mut locals = std::collections::HashMap::new();
    for c in LOCAL.captures_iter(text) {
      if let MetaValue::Literal(uuid) = Self::uuid_expr(&c["expr"], &locals) {
        locals.insert(c["name"].to_owned(), uuid);
      }
    }
    let line = |offset: usize| text[..offset].matches('\n').count() + 1;
    let mut dependencies: Vec<_> = CALL
      .captures_iter(text)
      .map(|c| MetaDependency {
        line: line(c.get(0).expect("whole match").start()),
        uuid: Some(Self::uuid_expr(&c["uuid"], &locals)),
        name: None,
        range: Self::range_expr(&c["range"]),
      })
      .collect();
    dependencies.extend(TABLE.captures_iter(text).map(|c| {
      let whole = c.get(0).expect("whole match");
      let mut dependency = MetaDependency {
        line: line(whole.start()),
        uuid: None,
        name: None,
        range: MetaValue::Literal(c["range"].to_owned()),
      };
      for field in FIELD.captures_iter(whole.as_str()) {
        let value = field["value"].to_owned();
        match &field["key"] {
          "name" => dependency.name = Some(value),
          _ => dependency.uuid = Some(MetaValue::Literal(value)),
        }
      }
      dependency
    }));
    dependencies.sort_by_key(|d| d.line);
    Self {
      uuid: UUID
        .captures(text)
        .map(|c| Self::uuid_expr(&c["expr"], &locals)),
      version: VERSION
        .captures(text)
        .map(|c| Self::version_expr(&c["expr"])),
      dependencies,
    }
  }

  fn key(name: &str) -> regex::Regex {
    regex::Regex::new(&format!(
      r"(?m)^[ \t]*{name}[ \t]*=[ \t]*(?<expr>.+?)[ \t]*,?[ \t]*(?:--.*)?$"
    ))
    .expect("valid")
  }

  fn uuid_expr(expr: &str, locals: &std::collections::HashMap<String, String>) -> MetaValue {
    static CALL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"^(?:mms\.)?Uuid[.:](?:from_string|new)\(\s*(?<arg>.+?)\s*\)$")
        .expect("valid")
    });
    if let Some(literal) = Self::string(expr) {
      return MetaValue::Literal(literal.to_owned());
    }
    if let Some(c) = CALL.captures(expr) {
      return Self::uuid_expr(&c["arg"], locals);
    }
    locals.get(expr).map_or_else(
      || MetaValue::Unresolved(expr.to_owned()),
      |uuid| MetaValue::Literal(uuid.clone()),
    )
  }

  fn version_expr(expr: &str) -> MetaValue {
    static PARSE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r#"^(?:mms\.)?Version[.:]parse\(\s*"(?<v>[^"]*)"\s*\)$"#).expect("valid")
    });
    static NEW: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r"^(?:mms\.)?Version[.:]new\(\s*(?<a>\d+)\s*,\s*(?<b>\d+)\s*,\s*(?<c>\d+)\s*\)$",
      )
      .expect("valid")
    });
    if let Some(literal) = Self::string(expr) {
      MetaValue::Literal(literal.to_owned())
    } else if let Some(c) = PARSE.captures(expr) {
      MetaValue::Literal(c["v"].to_owned())
    } else if let Some(c) = NEW.captures(expr) {
      MetaValue::Literal(format!("{}.{}.{}", &c["a"], &c["b"], &c["c"]))
    } else if expr == "mms.app.version" {
      MetaValue::Dynamic(expr.to_owned())
    } else {
      MetaValue::Unresolved(expr.to_owned())
    }
  }

  fn range_expr(expr: &str) -> MetaValue {
    static PARSE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r#"^(?:mms\.)?VersionRange[.:]parse\(\s*"(?<r>[^"]*)"\s*\)$"#)
        .expect("valid")
    });
    static ANY: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"^(?:mms\.)?VersionRange[.:]any\w*\(\s*\)$").expect("valid")
    });
    if let Some(literal) = Self::string(expr) {
      MetaValue::Literal(literal.to_owned())
    } else if let Some(c) = PARSE.captures(expr) {
      MetaValue::Literal(c["r"].to_owned())
    } else if ANY.is_match(expr) {
      MetaValue::Dynamic(expr.to_owned())
    } else {
      MetaValue::Unresolved(expr.to_owned())
    }
  }

  fn string(expr: &str) -> Option<&str> {
    expr
      .strip_prefix('"')
      .and_then(|s| s.strip_suffix('"'))
      .filter(|s| !s.contains('"'))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn literal(value: &str) -> MetaValue {
    MetaValue::Literal(value.to_owned())
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
        MetaDependency {
          line: 8,
          uuid: Some(literal("15cac520-303e-43d6-bce5-5d977134340a")),
          name: None,
          range: MetaValue::Literal(">=1.0 <2".into()),
        },
        MetaDependency {
          line: 9,
          uuid: Some(MetaValue::Unresolved("other".into())),
          name: None,
          range: MetaValue::Dynamic("mms.VersionRange:any_with_prerelease()".into()),
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
      Some(MetaValue::Dynamic("mms.app.version".into()))
    );
  }
}
