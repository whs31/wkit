mod bin;
mod build_type;
mod cargo_build;
mod conan_build;
mod conan_value;

pub use self::{
  bin::Bin, build_type::BuildType, cargo_build::CargoBuild, conan_build::ConanBuild,
  conan_value::ConanValue,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BuildSection {
  pub system: Option<wkit_core::detect::BuildSystem>,
  pub build_type: BuildType,
  pub conan: ConanBuild,
  pub cargo: CargoBuild,
}
