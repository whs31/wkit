#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum InstallMethod {
  #[display("Debian package")]
  Deb,

  #[display("Arch package")]
  Arch,

  #[display("files")]
  Files,
}
