#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(untagged)]
pub enum ConanValue {
  #[display("{}", if *_0 { "True" } else { "False" })]
  Bool(bool),

  Int(i64),

  String(String),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn booleans_use_python_spelling() {
    assert_eq!(ConanValue::Bool(true).to_string(), "True");
    assert_eq!(ConanValue::Bool(false).to_string(), "False");
    assert_eq!(ConanValue::Int(4).to_string(), "4");
    assert_eq!(ConanValue::String("gnu23".into()).to_string(), "gnu23");
  }
}
