use std::ffi::{OsStr, OsString};

use crate::{Failure, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct ProcessBuilder {
  program: OsString,
  args: Vec<OsString>,
  cwd: Option<std::path::PathBuf>,
  env: std::collections::BTreeMap<OsString, Option<OsString>>,
}

impl ProcessBuilder {
  pub fn new(program: impl AsRef<OsStr>) -> Self {
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

  pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
    self.args.push(arg.as_ref().to_owned());
    self
  }

  pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
    self
      .args
      .extend(args.into_iter().map(|a| a.as_ref().to_owned()));
    self
  }

  pub fn cwd(mut self, cwd: impl AsRef<std::path::Path>) -> Self {
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

  pub fn get_cwd(&self) -> Option<&std::path::Path> {
    self.cwd.as_deref()
  }

  pub fn get_env(&self, key: impl AsRef<OsStr>) -> Option<&OsStr> {
    self.env.get(key.as_ref()).and_then(Option::as_deref)
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
    self.check(status, &[])
  }

  pub fn status_unchecked(&self) -> Result<std::process::ExitStatus> {
    self.command().status().map_err(|e| self.spawn_error(e))
  }

  pub fn output(&self) -> Result<std::process::Output> {
    let output = self.output_unchecked()?;
    self.check(output.status, &output.stderr)?;
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
    let (reader, writer) = std::io::pipe()?;
    let mut command = self.command();
    command
      .stdin(std::process::Stdio::null())
      .stdout(writer.try_clone()?)
      .stderr(writer);
    let mut child = command.spawn().map_err(|e| self.spawn_error(e))?;
    drop(command);
    let mut reader = std::io::BufReader::new(reader);
    let mut buffer = Vec::new();
    while std::io::BufRead::read_until(&mut reader, b'\n', &mut buffer)? > 0 {
      let line = String::from_utf8_lossy(&buffer);
      let line = line.trim_end_matches(['\n', '\r']);
      on_line(line.rsplit('\r').next().unwrap_or(line));
      buffer.clear();
    }
    Ok(child.wait()?)
  }

  pub fn check(&self, status: std::process::ExitStatus, stderr: &[u8]) -> Result<()> {
    if status.success() {
      return Ok(());
    }
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim_end();
    let section = if stderr.is_empty() {
      String::new()
    } else {
      format!("\n--- stderr\n{stderr}")
    };
    Err(
      Failure::new(
        format!("process didn't exit successfully: `{self}` ({status}){section}"),
        status.code().unwrap_or(Failure::INTERNAL),
      )
      .into(),
    )
  }

  fn spawn_error(&self, source: std::io::Error) -> anyhow::Error {
    let cwd_exists = self.cwd.as_ref().is_none_or(|cwd| cwd.is_dir());
    if source.kind() == std::io::ErrorKind::NotFound && cwd_exists {
      anyhow::anyhow!("could not find `{}` in PATH", self.program.display())
    } else {
      anyhow::Error::new(source).context(format!("could not execute `{self}`"))
    }
  }
}

impl std::fmt::Display for ProcessBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let words: Vec<_> = std::iter::once(&self.program)
      .chain(&self.args)
      .map(|word| quote(&word.to_string_lossy()))
      .collect();
    f.write_str(&words.join(" "))
  }
}

pub fn quote(word: &str) -> String {
  let plain = !word.is_empty()
    && word
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || "-_./=:+,@%".contains(c));
  if plain {
    word.to_owned()
  } else {
    format!("'{}'", word.replace('\'', r"'\''"))
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
      .args(["hello world", "", "it's"]);
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
    assert_eq!(Failure::exit_code(&err), 3);
    let message = err.to_string();
    assert!(message.contains("exit status: 3"), "{message}");
    assert!(message.ends_with("--- stderr\noops"), "{message}");
  }

  #[test]
  fn missing_program_is_reported_by_name() {
    let err = ProcessBuilder::new("wkit-definitely-not-installed")
      .status()
      .unwrap_err();
    assert_eq!(
      err.to_string(),
      "could not find `wkit-definitely-not-installed` in PATH"
    );
  }

  #[cfg(unix)]
  #[test]
  fn output_captures_stdout() {
    let printf = ProcessBuilder::new("printf").arg("hi");
    assert_eq!(printf.output().unwrap().stdout, b"hi");
  }
}
