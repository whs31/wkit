use crate::mms::MetaValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDependency {
  pub line: usize,
  pub uuid: Option<MetaValue>,
  pub name: Option<String>,
  pub range: MetaValue,
}

impl MetaDependency {
  pub fn label(&self) -> String {
    match (&self.name, &self.uuid) {
      (Some(name), _) => format!("`{name}`"),
      (None, Some(MetaValue::Literal(uuid))) => uuid.clone(),
      (None, Some(MetaValue::Dynamic(expr) | MetaValue::Unresolved(expr))) => format!("`{expr}`"),
      (None, None) => "dependency".to_owned(),
    }
  }
}
