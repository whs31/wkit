use std::io::Write as _;

use wkit_core::{ProcessBuilder, Shell, shell::Verbosity};
use wkit_manifest::BuildType;

use crate::{BuildLog, BuiltPackage, Error, Event, PackageRef, Result};

#[derive(Debug)]
pub struct Reporter<'a> {
  shell: &'a mut Shell,
  log: BuildLog,
  build_type: BuildType,
  file: Option<(std::path::PathBuf, std::io::BufWriter<std::fs::File>)>,
  bar: Option<indicatif::ProgressBar>,
  stepping: bool,
  packages: Vec<BuiltPackage>,
}

impl<'a> Reporter<'a> {
  pub fn new(
    shell: &'a mut Shell,
    root: PackageRef,
    build_type: BuildType,
    log_path: Option<std::path::PathBuf>,
  ) -> Result<Self> {
    let file = match log_path {
      Some(path) => {
        let file = path
          .parent()
          .map_or(Ok(()), std::fs::create_dir_all)
          .and_then(|()| std::fs::File::create(&path))
          .map_err(|source| wkit_core::Error::Write {
            path: path.clone(),
            source,
          })?;
        Some((path, std::io::BufWriter::new(file)))
      }
      None => None,
    };
    Ok(Self {
      shell,
      log: BuildLog::new(root),
      build_type,
      file,
      bar: None,
      stepping: false,
      packages: Vec::new(),
    })
  }

  pub fn start_build(&mut self) {
    self.log.start_build();
  }

  pub fn run(&mut self, process: &ProcessBuilder) -> Result<()> {
    self.shell.verbose(|s| s.status("Running", process))?;
    if let Some((_, file)) = &mut self.file {
      writeln!(file, "$ {process}")?;
    }
    let mut failure = None;
    let status = process.stream(|line| {
      if let Err(e) = self.line(line) {
        failure.get_or_insert(e);
      }
    })?;
    for event in self.log.finish() {
      self.apply(event)?;
    }
    if let Some((_, file)) = &mut self.file {
      file.flush()?;
    }
    if let Some(e) = failure {
      return Err(e);
    }
    if status.success() {
      Ok(())
    } else {
      self.clear();
      Err(self.failure(process, status))
    }
  }

  pub fn finish(mut self) -> Vec<BuiltPackage> {
    self.clear();
    self.packages
  }

  fn line(&mut self, line: &str) -> Result<()> {
    if let Some((_, file)) = &mut self.file {
      writeln!(file, "{line}")?;
    }
    let verbose = self.shell.verbosity() == Verbosity::Verbose;
    if verbose {
      self.shell.passthrough(line)?;
    }
    for event in self.log.feed(line) {
      if !verbose || matches!(event, Event::BuildDir(..)) {
        self.apply(event)?;
      }
    }
    Ok(())
  }

  fn apply(&mut self, event: Event) -> Result<()> {
    match event {
      Event::Phase { status, message } => {
        let bar = self.shell.transient(status);
        bar.set_message(message);
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        self.bar = Some(bar);
        self.stepping = false;
      }
      Event::Downloading(package) => self.shell.status("Downloading", package)?,
      Event::CompilingDependency(package) => self.shell.status("Compiling", package)?,
      Event::Configuring(package) => self.shell.status(
        "Configuring",
        format_args!("{package} ({})", self.build_type),
      )?,
      Event::Compiling(package) => self
        .shell
        .status("Compiling", format_args!("{package} ({})", self.build_type))?,
      Event::Step {
        current,
        total,
        target,
      } => {
        if !self.stepping {
          self.bar = Some(self.shell.progress("Building", total));
          self.stepping = true;
        }
        if let Some(bar) = &self.bar {
          bar.set_length(total);
          bar.set_position(current);
          bar.set_message(format!(": {target}"));
        }
      }
      Event::Diagnostic(text) => self.shell.passthrough(text)?,
      Event::Warning(text) => self.shell.warn(text)?,
      Event::BuildDir(package, build_dir) => {
        self.packages.retain(|p| p.package != package);
        self.packages.push(BuiltPackage { package, build_dir });
      }
    }
    Ok(())
  }

  fn clear(&mut self) {
    if let Some(bar) = self.bar.take() {
      bar.finish_and_clear();
    }
    self.stepping = false;
  }

  fn failure(&self, process: &ProcessBuilder, status: std::process::ExitStatus) -> Error {
    let mut errors = Vec::new();
    if self.log.in_dependency() {
      errors.extend(self.log.tail().map(ToOwned::to_owned));
      errors.extend(self.log.errors().iter().cloned());
    } else if self.log.diagnostics() == 0 {
      errors.extend(self.log.errors().iter().cloned());
    }
    if errors.is_empty() && self.log.diagnostics() == 0 {
      errors.push(format!("`{process}` failed ({status})"));
    }
    Error::Failed {
      package: self.log.current().to_string(),
      errors,
      log: self.file.as_ref().map(|(path, _)| path.clone()),
    }
  }
}
