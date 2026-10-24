use wkit_core::{Result, Shell};
use wkit_manifest::Project;

use crate::{ClangFormat, Mode, Rustfmt};

#[derive(Debug, Clone)]
pub struct Formatter {
  pub rustfmt: Option<Rustfmt>,
  pub clang_format: Option<ClangFormat>,
}

impl Formatter {
  pub fn new(project: &Project) -> Result<Self> {
    Ok(Self {
      rustfmt: Rustfmt::new(project)?,
      clang_format: ClangFormat::new(project),
    })
  }

  pub fn is_empty(&self) -> bool {
    self.rustfmt.is_none() && self.clang_format.is_none()
  }

  pub fn run(&self, shell: &mut Shell, mode: Mode, jobs: usize) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if let Some(rustfmt) = &self.rustfmt {
      files.extend(rustfmt.run(shell, mode)?);
    }
    if let Some(clang_format) = &self.clang_format {
      files.extend(clang_format.run(shell, mode, jobs)?);
    }
    Ok(files)
  }
}
