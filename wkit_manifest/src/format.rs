mod cpp_format;
mod extra_format;
mod rust_format;

pub use self::{cpp_format::CppFormat, extra_format::ExtraFormat, rust_format::RustFormat};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FormatSection {
  pub rust: RustFormat,
  pub cpp: CppFormat,
  pub qmlformat: ExtraFormat,
  pub black: ExtraFormat,
  pub stylua: ExtraFormat,
}
