use std::{
  borrow::Cow,
  collections::BTreeMap,
  ffi::{OsStr, OsString},
  fmt, io,
  path::{Path, PathBuf},
  process::{Command, ExitStatus, Output, Stdio},
};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct ProcessBuilder {
  program: OsString,
  args: Vec<OsString>,
  cwd: Option<PathBuf>,
  env: BTreeMap<OsString, Option<OsString>>,
}

impl ProcessBuilder {
  pub fn new(program: impl AsRef<OsStr>) -> Self {
    Self {
      program: program.as_ref().to_owned(),
      args: Vec::new(),
      cwd: None,
      env: BTreeMap::new(),
    }
  }

  pub fn shell(command: &str) -> Self {
    if cfg!(windows) {
      Self::new("cmd").args(["/C", command])
    } else {
      Self::new("sh").args(["-c", command])
    }
  }

  pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
    self.args.push(arg.as_ref().to_owned());
    self
  }

  pub fn args<I, S>(mut self, args: I) -> Self
  where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
  {
    self
      .args
      .extend(args.into_iter().map(|a| a.as_ref().to_owned()));
    self
  }

  pub fn cwd(mut self, cwd: impl AsRef<Path>) -> Self {
    self.cwd = Some(cwd.as_ref().to_owned());
    self
  }

  pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
    self
      .env
      .insert(key.as_ref().to_owned(), Some(value.as_ref().to_owned()));
    self
  }

  pub fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
    self.env.insert(key.as_ref().to_owned(), None);
    self
  }

  pub fn get_program(&self) -> &OsStr {
    &self.program
  }

  pub fn get_args(&self) -> &[OsString] {
    &self.args
  }

  pub fn get_cwd(&self) -> Option<&Path> {
    self.cwd.as_deref()
  }

  pub fn get_env(&self, key: impl AsRef<OsStr>) -> Option<&OsStr> {
    self.env.get(key.as_ref()).and_then(Option::as_deref)
  }

  pub fn command(&self) -> Command {
    let mut command = Command::new(&self.program);
    command.args(&self.args);
    if let Some(cwd) = &self.cwd {
      command.current_dir(cwd);
    }
    for (key, value) in &self.env {
      match value {
        Some(value) => command.env(key, value),
        None => command.env_remove(key),
      };
    }
    command
  }

  pub fn status(&self) -> Result<()> {
    let status = self.status_unchecked()?;
    self.check(status, String::new())
  }

  pub fn status_unchecked(&self) -> Result<ExitStatus> {
    self.command().status().map_err(|e| self.spawn_error(e))
  }

  pub fn output(&self) -> Result<Output> {
    let output = self.output_unchecked()?;
    self.check(
      output.status,
      String::from_utf8_lossy(&output.stderr).into_owned(),
    )?;
    Ok(output)
  }

  pub fn output_unchecked(&self) -> Result<Output> {
    self
      .command()
      .stdin(Stdio::null())
      .output()
      .map_err(|e| self.spawn_error(e))
  }

  fn check(&self, status: ExitStatus, stderr: String) -> Result<()> {
    if status.success() {
      Ok(())
    } else {
      Err(Error::ProcessFailed {
        command: self.to_string(),
        status,
        stderr,
      })
    }
  }

  fn spawn_error(&self, source: io::Error) -> Error {
    if source.kind() == io::ErrorKind::NotFound && self.cwd.as_ref().is_none_or(|cwd| cwd.is_dir())
    {
      Error::ProgramNotFound {
        program: self.program.to_string_lossy().into_owned(),
      }
    } else {
      Error::Spawn {
        command: self.to_string(),
        source,
      }
    }
  }
}

impl fmt::Display for ProcessBuilder {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&quote(&self.program.to_string_lossy()))?;
    for arg in &self.args {
      write!(f, " {}", quote(&arg.to_string_lossy()))?;
    }
    Ok(())
  }
}

fn quote(arg: &str) -> Cow<'_, str> {
  let plain = !arg.is_empty()
    && arg
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || "-_./=:+,@%".contains(c));
  if plain {
    Cow::Borrowed(arg)
  } else {
    Cow::Owned(format!("'{}'", arg.replace('\'', r"'\''")))
  }
}

pub fn find_program(name: impl AsRef<OsStr>) -> Option<PathBuf> {
  which::which(name).ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display_quotes_only_when_needed() {
    let p = ProcessBuilder::new("conan")
      .args(["build", ".", "-o", "&:test=True"])
      .arg("hello world")
      .arg("")
      .arg("it's");
    assert_eq!(
      p.to_string(),
      r"conan build . -o '&:test=True' 'hello world' '' 'it'\''s'"
    );
  }

  #[test]
  fn env_overrides_are_recorded() {
    let p = ProcessBuilder::new("x").env("A", "1").env_remove("B");
    assert_eq!(p.get_env("A"), Some(OsStr::new("1")));
    assert_eq!(p.get_env("B"), None);
  }

  #[cfg(unix)]
  #[test]
  fn failing_process_reports_status_and_stderr() {
    let err = ProcessBuilder::shell("echo oops >&2; exit 3")
      .output()
      .unwrap_err();
    assert_eq!(err.exit_code(), Some(3));
    let message = err.to_string();
    assert!(message.contains("exit status: 3"), "{message}");
    assert!(message.ends_with("--- stderr\noops"), "{message}");
  }

  #[test]
  fn missing_program_is_reported_by_name() {
    let err = ProcessBuilder::new("wkit-definitely-not-installed")
      .status()
      .unwrap_err();
    assert!(
      matches!(err, Error::ProgramNotFound { ref program } if program == "wkit-definitely-not-installed")
    );
  }

  #[cfg(unix)]
  #[test]
  fn output_captures_stdout() {
    let out = ProcessBuilder::new("printf").arg("hi").output().unwrap();
    assert_eq!(out.stdout, b"hi");
  }
}
