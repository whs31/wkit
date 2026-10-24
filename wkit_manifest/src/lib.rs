mod de;
mod manifest;

pub mod build;
pub mod check;
pub mod clean;
pub mod conan;
pub mod error;
pub mod format;
pub mod install;
pub mod package;
pub mod project;
pub mod task;
pub mod test;
pub mod tools;
pub mod version;

pub use self::{
  build::{Bin, BuildSection, BuildType},
  check::CheckSection,
  clean::CleanSection,
  conan::ConanSection,
  error::ManifestError,
  format::FormatSection,
  install::InstallSection,
  manifest::{Manifest, Parsed},
  package::{FileMapping, FileMode, PackageSection},
  project::{Project, ProjectSection},
  task::{Command, CustomCheck, Task},
  test::{TestRunner, TestSection, TestSuite},
  tools::Tools,
  version::{VersionFile, VersionFileKind, VersionSection},
};

pub const MANIFEST_FILE: &str = "wkit.toml";
