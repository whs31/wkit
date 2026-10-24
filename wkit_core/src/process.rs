mod shell_quoted;

pub use self::shell_quoted::ShellQuoted;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct ProcessBuilder {
  program: std::ffi::OsString,
  args: Vec<std::ffi::OsString>,
  cwd: Option<std::path::PathBuf>,
  env: std::collections::BTreeMap<std::ffi::OsString, Option<std::ffi::OsString>>,
}

impl ProcessBuilder {
  pub fn new(program: impl AsRef<std::ffi::OsStr>) -> Self {
    Self {
      program: program.as_ref().to_owned(),
      args: Vec::new(),
      cwd: None,
      env: std::collections::BTreeMap::new(),
    }
  }

  pub fn shell(command: &str) -> Self {
    if cfg!(windows) {
      Self::new("cmd").args(["/C", command])
    } else {
      Self::new("sh").args(["-c", command])
    }
  }

  pub fn arg(mut self, arg: impl AsRef<std::ffi::OsStr>) -> Self {
    self.args.push(arg.as_ref().to_owned());
    self
  }

  pub fn args<I, S>(mut self, args: I) -> Self
  where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
  {
    self
      .args
      .extend(args.into_iter().map(|a| a.as_ref().to_owned()));
    self
  }

  pub fn cwd(mut self, cwd: impl AsRef<std::path::Path>) -> Self {
    self.cwd = Some(cwd.as_ref().to_owned());
    self
  }

  pub fn env(
    mut self,
    key: impl AsRef<std::ffi::OsStr>,
    value: impl AsRef<std::ffi::OsStr>,
  ) -> Self {
    self
      .env
      .insert(key.as_ref().to_owned(), Some(value.as_ref().to_owned()));
    self
  }

  pub fn env_remove(mut self, key: impl AsRef<std::ffi::OsStr>) -> Self {
    self.env.insert(key.as_ref().to_owned(), None);
    self
  }

  pub fn get_program(&self) -> &std::ffi::OsStr {
    &self.program
  }

  pub fn get_args(&self) -> &[std::ffi::OsString] {
    &self.args
  }

  pub fn get_cwd(&self) -> Option<&std::path::Path> {
    self.cwd.as_deref()
  }

  pub fn get_env(&self, key: impl AsRef<std::ffi::OsStr>) -> Option<&std::ffi::OsStr> {
    self.env.get(key.as_ref()).and_then(Option::as_deref)
  }

  pub fn program_path(&self) -> Option<std::path::PathBuf> {
    which::which(&self.program).ok()
  }

  pub fn command(&self) -> std::process::Command {
    let mut command = std::process::Command::new(&self.program);
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

  pub fn status_unchecked(&self) -> Result<std::process::ExitStatus> {
    self.command().status().map_err(|e| self.spawn_error(e))
  }

  pub fn output(&self) -> Result<std::process::Output> {
    let output = self.output_unchecked()?;
    self.check(
      output.status,
      String::from_utf8_lossy(&output.stderr).into_owned(),
    )?;
    Ok(output)
  }

  pub fn output_unchecked(&self) -> Result<std::process::Output> {
    self
      .command()
      .stdin(std::process::Stdio::null())
      .output()
      .map_err(|e| self.spawn_error(e))
  }

  pub fn stream(&self, mut on_line: impl FnMut(&str)) -> Result<std::process::ExitStatus> {
    let (reader, writer) = std::io::pipe().map_err(|e| self.spawn_error(e))?;
    let mut command = self.command();
    command
      .stdin(std::process::Stdio::null())
      .stdout(writer.try_clone().map_err(|e| self.spawn_error(e))?)
      .stderr(writer);
    let mut child = command.spawn().map_err(|e| self.spawn_error(e))?;
    drop(command);
    let mut reader = std::io::BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
      buffer.clear();
      let read = std::io::BufRead::read_until(&mut reader, b'\n', &mut buffer)
        .map_err(|e| self.spawn_error(e))?;
      if read == 0 {
        break;
      }
      let line = String::from_utf8_lossy(&buffer);
      let line = line.trim_end_matches(['\n', '\r']);
      on_line(line.rsplit('\r').next().unwrap_or(line));
    }
    child.wait().map_err(|e| self.spawn_error(e))
  }

  fn check(&self, status: std::process::ExitStatus, stderr: String) -> Result<()> {
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

  fn spawn_error(&self, source: std::io::Error) -> Error {
    let cwd_exists = self.cwd.as_ref().is_none_or(|cwd| cwd.is_dir());
    if source.kind() == std::io::ErrorKind::NotFound && cwd_exists {
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

impl std::fmt::Display for ProcessBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", ShellQuoted(&self.program.to_string_lossy()))?;
    for arg in &self.args {
      write!(f, " {}", ShellQuoted(&arg.to_string_lossy()))?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(unix)]
  #[test]
  fn stream_merges_stdout_and_stderr_in_order() {
    let mut lines = Vec::new();
    let status = ProcessBuilder::shell("echo out; echo err >&2; printf 'a\\rb\\n'; exit 3")
      .stream(|line| lines.push(line.to_owned()))
      .unwrap();
    assert_eq!(status.code(), Some(3));
    assert_eq!(lines, ["out", "err", "b"]);
  }

  #[test]
  fn display_is_a_pasteable_command_line() {
    let p = ProcessBuilder::new("conan")
      .args(["build", ".", "-o", "&:test=True"])
      .arg("hello world");
    assert_eq!(
      p.to_string(),
      "conan build . -o '&:test=True' 'hello world'"
    );
  }

  #[test]
  fn env_overrides_are_recorded() {
    let p = ProcessBuilder::new("x").env("A", "1").env_remove("B");
    assert_eq!(p.get_env("A"), Some(std::ffi::OsStr::new("1")));
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
    let missing = ProcessBuilder::new("wkit-definitely-not-installed");
    assert_eq!(missing.program_path(), None);
    let err = missing.status().unwrap_err();
    assert!(
      matches!(err, Error::ProgramNotFound { ref program } if program == "wkit-definitely-not-installed")
    );
  }

  #[cfg(unix)]
  #[test]
  fn output_captures_stdout() {
    let printf = ProcessBuilder::new("printf").arg("hi");
    assert!(printf.program_path().is_some());
    assert_eq!(printf.output().unwrap().stdout, b"hi");
  }
}
