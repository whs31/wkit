pub mod detect;
pub mod error;
pub mod fs;
pub mod lang;
pub mod process;
pub mod shell;
pub mod tool;
pub mod version;

pub use self::{
  error::{Error, Result},
  lang::Language,
  process::ProcessBuilder,
  shell::Shell,
};
