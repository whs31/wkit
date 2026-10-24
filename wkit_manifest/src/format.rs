mod cpp_format;
mod rust_format;

pub use self::{cpp_format::CppFormat, rust_format::RustFormat};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FormatSection {
  pub rust: RustFormat,
  pub cpp: CppFormat,
}
