mod build_log;
mod build_options;
mod build_outcome;
mod builder;
mod built_package;
mod cargo;
mod cmake;
mod compile_commands;
mod conan;
mod conan_presets;
mod conan_workspace;
mod error;
mod package_ref;
mod remote_sync;
mod reporter;
mod workspace_package;

pub use self::{
  build_log::{BuildLog, Event},
  build_options::BuildOptions,
  build_outcome::BuildOutcome,
  builder::Builder,
  built_package::BuiltPackage,
  cargo::Cargo,
  cmake::CMake,
  compile_commands::CompileCommands,
  conan::Conan,
  conan_presets::ConanPresets,
  conan_workspace::ConanWorkspace,
  error::Error,
  package_ref::PackageRef,
  remote_sync::RemoteSync,
  reporter::Reporter,
  workspace_package::WorkspacePackage,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
