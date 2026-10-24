mod bin;
mod build_type;
mod cargo_build;
mod conan_build;
mod conan_value;

pub use self::{
  bin::Bin, build_type::BuildType, cargo_build::CargoBuild, conan_build::ConanBuild,
  conan_value::ConanValue,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BuildSection {
  pub system: Option<wkit_core::detect::BuildSystem>,
  pub link_compile_commands: bool,
  pub conan: ConanBuild,
  pub cargo: CargoBuild,
}

impl Default for BuildSection {
  fn default() -> Self {
    Self {
      system: None,
      link_compile_commands: true,
      conan: ConanBuild::default(),
      cargo: CargoBuild::default(),
    }
  }
}
