pub mod detect;
pub mod error;
pub mod fs;
pub mod lang;
pub mod process;
pub mod shell;
pub mod tool;
pub mod version;

pub use error::{Error, Result};
pub use lang::Language;
pub use process::ProcessBuilder;
pub use shell::Shell;
