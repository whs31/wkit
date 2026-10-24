mod build_log;
mod build_options;
mod build_outcome;
mod builder;
mod built_package;
mod cargo;
mod cmake;
mod conan;
mod error;
mod package_ref;
mod reporter;

pub use self::{
  build_log::{BuildLog, Event},
  build_options::BuildOptions,
  build_outcome::BuildOutcome,
  builder::Builder,
  built_package::BuiltPackage,
  cargo::Cargo,
  cmake::CMake,
  conan::Conan,
  error::Error,
  package_ref::PackageRef,
  reporter::Reporter,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
