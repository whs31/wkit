mod bytes;
mod color_choice;
mod elapsed;
mod sink;
pub mod style;
mod verbosity;

use self::sink::Sink;
pub use self::{bytes::Bytes, color_choice::ColorChoice, elapsed::Elapsed, verbosity::Verbosity};

pub const STATUS_WIDTH: usize = 12;

pub struct Shell {
  sink: Sink,
  verbosity: Verbosity,
  color: ColorChoice,
  progress: Option<indicatif::ProgressBar>,
}

impl std::fmt::Debug for Shell {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Shell")
      .field("verbosity", &self.verbosity)
      .field("color", &self.color)
      .finish_non_exhaustive()
  }
}

impl Default for Shell {
  fn default() -> Self {
    Self::new()
  }
}

impl Shell {
  pub fn new() -> Self {
    Self::with(Verbosity::Normal, ColorChoice::Auto)
  }

  pub fn with(verbosity: Verbosity, color: ColorChoice) -> Self {
    Self {
      sink: Sink::std(color),
      verbosity,
      color,
      progress: None,
    }
  }

  pub fn from_write(out: Box<dyn std::io::Write>) -> Self {
    Self {
      sink: Sink::write(out),
      verbosity: Verbosity::Normal,
      color: ColorChoice::Never,
      progress: None,
    }
  }

  pub fn verbosity(&self) -> Verbosity {
    self.verbosity
  }

  pub fn set_verbosity(&mut self, verbosity: Verbosity) {
    self.verbosity = verbosity;
  }

  pub fn color_choice(&self) -> ColorChoice {
    self.color
  }

  pub fn set_color_choice(&mut self, color: ColorChoice) {
    self.color = color;
    if matches!(self.sink, Sink::Std { .. }) {
      self.sink = Sink::std(color);
    }
  }

  pub fn err_supports_color(&self) -> bool {
    self.sink.err_supports_color()
  }

  pub fn is_err_tty(&self) -> bool {
    self.sink.is_err_tty()
  }

  pub fn out(&mut self) -> &mut dyn std::io::Write {
    self.sink.out()
  }

  pub fn err(&mut self) -> &mut dyn std::io::Write {
    self.sink.err()
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
    self.write_err(&format!(
      "{style}{status:>STATUS_WIDTH$}{style:#} {message}\n"
    ))
  }

  pub fn verbose(
    &mut self,
    f: impl FnOnce(&mut Self) -> std::io::Result<()>,
  ) -> std::io::Result<()> {
    if self.verbosity == Verbosity::Verbose {
      f(self)
    } else {
      Ok(())
    }
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
    self.status("Finished", format_args!("{what} in {}", Elapsed(elapsed)))
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
    let target = if self.verbosity == Verbosity::Quiet || !self.is_err_tty() {
      indicatif::ProgressDrawTarget::hidden()
    } else {
      indicatif::ProgressDrawTarget::stderr()
    };
    let prefix = if self.err_supports_color() {
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
    let progress = self.progress.clone();
    let err = self.err();
    match progress {
      Some(bar) if !bar.is_finished() && !bar.is_hidden() => {
        bar.suspend(|| err.write_all(text.as_bytes()))
      }
      _ => err.write_all(text.as_bytes()),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, io::Write, rc::Rc};

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

  impl Capture {
    fn text(&self) -> String {
      String::from_utf8(self.0.borrow().clone()).unwrap()
    }
  }

  fn shell() -> (Shell, Capture) {
    let capture = Capture::default();
    (Shell::from_write(Box::new(capture.clone())), capture)
  }

  #[test]
  fn status_is_right_aligned_like_cargo() {
    let (mut shell, capture) = shell();
    shell.status("Formatting", "42 files").unwrap();
    shell.status("Bumping", "1.0.0 -> 1.0.1").unwrap();
    assert_eq!(
      capture.text(),
      "  Formatting 42 files\n     Bumping 1.0.0 -> 1.0.1\n"
    );
  }

  #[test]
  fn quiet_suppresses_everything_but_errors() {
    let (mut shell, capture) = shell();
    shell.set_verbosity(Verbosity::Quiet);
    shell.status("Building", "x").unwrap();
    shell.warn("w").unwrap();
    shell.note("n").unwrap();
    shell.error("boom").unwrap();
    assert_eq!(capture.text(), "error: boom\n");
  }

  #[test]
  fn verbose_runs_only_in_verbose_mode() {
    let (mut shell, capture) = shell();
    shell.verbose(|s| s.status("Running", "`a`")).unwrap();
    shell.set_verbosity(Verbosity::Verbose);
    shell.verbose(|s| s.status("Running", "`b`")).unwrap();
    assert_eq!(capture.text(), "     Running `b`\n");
  }

  #[test]
  fn finished_reports_elapsed_time() {
    let (mut shell, capture) = shell();
    shell
      .finished("formatting", std::time::Duration::from_millis(1500))
      .unwrap();
    assert_eq!(capture.text(), "    Finished formatting in 1.50s\n");
  }
}
