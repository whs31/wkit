use std::io::{IsTerminal, Write};

pub use clap::ColorChoice;

pub mod style {
  use anstyle::{AnsiColor, Style};

  pub const HEADER: Style = AnsiColor::Green.on_default().bold();
  pub const PROGRESS: Style = AnsiColor::Cyan.on_default().bold();
  pub const ERROR: Style = AnsiColor::Red.on_default().bold();
  pub const WARN: Style = AnsiColor::Yellow.on_default().bold();
  pub const NOTE: Style = AnsiColor::Cyan.on_default().bold();
  pub const DIM: Style = Style::new().dimmed();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
  Quiet,
  #[default]
  Normal,
  Verbose,
}

pub struct Shell {
  out: Box<dyn Write>,
  err: Box<dyn Write>,
  err_color: bool,
  err_tty: bool,
  verbosity: Verbosity,
  color: ColorChoice,
  progress: Option<indicatif::ProgressBar>,
}

impl Shell {
  pub fn new(verbosity: Verbosity, color: ColorChoice) -> Self {
    let choice = match color {
      ColorChoice::Auto => anstream::ColorChoice::Auto,
      ColorChoice::Always => anstream::ColorChoice::Always,
      ColorChoice::Never => anstream::ColorChoice::Never,
    };
    let err = anstream::AutoStream::new(std::io::stderr(), choice);
    Self {
      err_color: err.current_choice() != anstream::ColorChoice::Never,
      err_tty: std::io::stderr().is_terminal(),
      out: Box::new(anstream::AutoStream::new(std::io::stdout(), choice)),
      err: Box::new(err),
      verbosity,
      color,
      progress: None,
    }
  }

  #[cfg(test)]
  pub fn capture(err: impl Write + 'static) -> Self {
    Self {
      out: Box::new(std::io::sink()),
      err: Box::new(err),
      err_color: false,
      err_tty: false,
      verbosity: Verbosity::Normal,
      color: ColorChoice::Never,
      progress: None,
    }
  }

  #[cfg(test)]
  pub fn sink() -> Self {
    Self::capture(std::io::sink())
  }

  pub fn is_verbose(&self) -> bool {
    self.verbosity == Verbosity::Verbose
  }

  pub fn color_choice(&self) -> ColorChoice {
    self.color
  }

  pub fn out(&mut self) -> &mut dyn Write {
    &mut self.out
  }

  pub fn err(&mut self) -> &mut dyn Write {
    &mut self.err
  }

  pub fn status(
    &mut self,
    status: impl std::fmt::Display,
    message: impl std::fmt::Display,
  ) -> std::io::Result<()> {
    self.status_with_style(status, message, &style::HEADER)
  }

  pub fn status_with_style(
    &mut self,
    status: impl std::fmt::Display,
    message: impl std::fmt::Display,
    style: &anstyle::Style,
  ) -> std::io::Result<()> {
    if self.verbosity == Verbosity::Quiet {
      return Ok(());
    }
    let status = status.to_string();
    self.write_err(&format!("{style}{status:>12}{style:#} {message}\n"))
  }

  pub fn verbose(
    &mut self,
    f: impl FnOnce(&mut Self) -> std::io::Result<()>,
  ) -> std::io::Result<()> {
    if self.is_verbose() { f(self) } else { Ok(()) }
  }

  pub fn error(&mut self, message: impl std::fmt::Display) -> std::io::Result<()> {
    self.labeled("error", message, &style::ERROR)
  }

  pub fn warn(&mut self, message: impl std::fmt::Display) -> std::io::Result<()> {
    if self.verbosity == Verbosity::Quiet {
      return Ok(());
    }
    self.labeled("warning", message, &style::WARN)
  }

  pub fn note(&mut self, message: impl std::fmt::Display) -> std::io::Result<()> {
    if self.verbosity == Verbosity::Quiet {
      return Ok(());
    }
    self.labeled("note", message, &style::NOTE)
  }

  pub fn finished(
    &mut self,
    what: impl std::fmt::Display,
    elapsed: std::time::Duration,
  ) -> std::io::Result<()> {
    self.status(
      "Finished",
      format_args!("{what} in {}", self::elapsed(elapsed)),
    )
  }

  pub fn passthrough(&mut self, line: impl std::fmt::Display) -> std::io::Result<()> {
    self.write_err(&format!("{line}\n"))
  }

  pub fn progress(&mut self, status: &str, len: u64) -> indicatif::ProgressBar {
    self.bar(status, Some(len), "[{bar:27}] {pos}/{len}{wide_msg}")
  }

  pub fn transient(&mut self, status: &str) -> indicatif::ProgressBar {
    self.bar(status, None, "{wide_msg}")
  }

  fn bar(&mut self, status: &str, len: Option<u64>, body: &str) -> indicatif::ProgressBar {
    if let Some(previous) = self.progress.take() {
      previous.finish_and_clear();
    }
    let target = if self.verbosity == Verbosity::Quiet || !self.err_tty {
      indicatif::ProgressDrawTarget::hidden()
    } else {
      indicatif::ProgressDrawTarget::stderr()
    };
    let prefix = if self.err_color {
      "{prefix:>12.cyan.bold}"
    } else {
      "{prefix:>12}"
    };
    let style = indicatif::ProgressStyle::with_template(&format!("{prefix} {body}"))
      .expect("progress template is valid")
      .progress_chars("=> ");
    let bar = indicatif::ProgressBar::with_draw_target(len, target)
      .with_style(style)
      .with_prefix(status.to_owned());
    self.progress = Some(bar.clone());
    bar
  }

  fn labeled(
    &mut self,
    label: &str,
    message: impl std::fmt::Display,
    style: &anstyle::Style,
  ) -> std::io::Result<()> {
    self.write_err(&format!("{style}{label}{style:#}: {message}\n"))
  }

  fn write_err(&mut self, text: &str) -> std::io::Result<()> {
    match &self.progress {
      Some(bar) if !bar.is_finished() && !bar.is_hidden() => {
        bar.suspend(|| self.err.write_all(text.as_bytes()))
      }
      _ => self.err.write_all(text.as_bytes()),
    }
  }
}

pub fn elapsed(elapsed: std::time::Duration) -> String {
  let secs = elapsed.as_secs();
  if secs >= 60 {
    format!("{}m {:02}s", secs / 60, secs % 60)
  } else {
    format!("{:.2}s", elapsed.as_secs_f64())
  }
}

pub fn plural(count: usize, word: &str) -> String {
  match word.strip_suffix('y') {
    _ if count == 1 => format!("1 {word}"),
    Some(stem) => format!("{count} {stem}ies"),
    None => format!("{count} {word}s"),
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, rc::Rc, time::Duration};

  use super::*;

  #[derive(Clone, Default)]
  struct Capture(Rc<RefCell<Vec<u8>>>);

  impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
      self.0.borrow_mut().extend_from_slice(buf);
      Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
      Ok(())
    }
  }

  fn shell(verbosity: Verbosity) -> (Shell, Capture) {
    let capture = Capture::default();
    let mut shell = Shell::capture(capture.clone());
    shell.verbosity = verbosity;
    (shell, capture)
  }

  fn text(capture: &Capture) -> String {
    anstream::adapter::strip_str(&String::from_utf8(capture.0.borrow().clone()).unwrap())
      .to_string()
  }

  #[test]
  fn status_is_right_aligned_like_cargo() {
    let (mut shell, capture) = shell(Verbosity::Normal);
    shell.status("Formatting", "42 files").unwrap();
    shell.status("Bumping", "1.0.0 -> 1.0.1").unwrap();
    shell
      .finished("formatting", Duration::from_millis(1500))
      .unwrap();
    assert_eq!(
      text(&capture),
      "  Formatting 42 files\n     Bumping 1.0.0 -> 1.0.1\n    Finished formatting in 1.50s\n"
    );
  }

  #[test]
  fn quiet_suppresses_everything_but_errors() {
    let (mut shell, capture) = shell(Verbosity::Quiet);
    shell.status("Building", "x").unwrap();
    shell.warn("w").unwrap();
    shell.note("n").unwrap();
    shell.error("boom").unwrap();
    assert_eq!(text(&capture), "error: boom\n");
  }

  #[test]
  fn verbose_runs_only_in_verbose_mode() {
    let (mut shell, capture) = shell(Verbosity::Normal);
    shell.verbose(|s| s.status("Running", "`a`")).unwrap();
    shell.verbosity = Verbosity::Verbose;
    shell.verbose(|s| s.status("Running", "`b`")).unwrap();
    assert_eq!(text(&capture), "     Running `b`\n");
  }

  #[test]
  fn elapsed_time_is_formatted_like_cargo() {
    assert_eq!(elapsed(Duration::from_millis(1234)), "1.23s");
    assert_eq!(elapsed(Duration::from_secs(125)), "2m 05s");
  }
}
