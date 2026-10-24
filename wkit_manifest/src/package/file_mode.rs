#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display, serde::Deserialize)]
#[display("{_0:04o}")]
#[serde(try_from = "String")]
pub struct FileMode(pub u32);

impl FileMode {
  pub const EXECUTABLE: Self = Self(0o755);
  pub const REGULAR: Self = Self(0o644);
}

impl TryFrom<String> for FileMode {
  type Error = String;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    u32::from_str_radix(&value, 8)
      .ok()
      .filter(|&mode| mode <= 0o7777)
      .map(Self)
      .ok_or_else(|| format!("`{value}` is not an octal file mode like \"755\""))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_octal_strings() {
    assert_eq!(FileMode::try_from("0755".to_owned()), Ok(FileMode(0o755)));
    assert_eq!(FileMode(0o644).to_string(), "0644");
    assert!(FileMode::try_from("rwx".to_owned()).is_err());
    assert!(FileMode::try_from("17777".to_owned()).is_err());
  }
}
