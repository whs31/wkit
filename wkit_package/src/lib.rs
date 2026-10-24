mod arch;
mod deb;
mod distro;
mod error;
mod file_source;
mod installer;
mod packaging;
mod privileged;
mod stage;
mod tool_run;
mod windows;

pub use self::{
  arch::{ArchPackager, SourceTarball, SrcInfo},
  deb::{CargoDeb, DebArchive, DebControl, DebPackager},
  distro::Distro,
  error::Error,
  file_source::FileSource,
  installer::{InstallMethod, Installer},
  packaging::Packaging,
  privileged::Privileged,
  stage::Stage,
  tool_run::ToolRun,
  windows::WindowsPackager,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
