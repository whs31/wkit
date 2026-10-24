use std::{
  fmt,
  io::{self, Write},
  str::FromStr,
  time::Duration,
};

use anstream::AutoStream;
use anstyle::Style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

pub const STATUS_WIDTH: usize = 12;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorChoice {
  #[default]
  Auto,
  Always,
  Never,
}

impl ColorChoice {
  fn to_stream(self) -> anstream::ColorChoice {
    match self {
      Self::Auto => anstream::ColorChoice::Auto,
      Self::Always => anstream::ColorChoice::Always,
      Self::Never => anstream::ColorChoice::Never,
    }
  }
}

impl FromStr for ColorChoice {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "auto" => Ok(Self::Auto),
      "always" => Ok(Self::Always),
      "never" => Ok(Self::Never),
      other => Err(format!(
        "argument for --color must be auto, always, or never, but found `{other}`"
      )),
    }
  }
}

enum Sink {
  Std {
    out: AutoStream<io::Stdout>,
    err: AutoStream<io::Stderr>,
  },
  Write(AutoStream<Box<dyn Write>>),
}

pub struct Shell {
  sink: Sink,
  verbosity: Verbosity,
  color: ColorChoice,
  progress: Option<ProgressBar>,
}

impl fmt::Debug for Shell {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
      sink: Self::std_sink(color),
      verbosity,
      color,
      progress: None,
    }
  }

  pub fn from_write(out: Box<dyn Write>) -> Self {
    Self {
      sink: Sink::Write(AutoStream::new(out, anstream::ColorChoice::Never)),
      verbosity: Verbosity::Normal,
      color: ColorChoice::Never,
      progress: None,
    }
  }

  fn std_sink(color: ColorChoice) -> Sink {
    Sink::Std {
      out: AutoStream::new(io::stdout(), color.to_stream()),
      err: AutoStream::new(io::stderr(), color.to_stream()),
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
      self.sink = Self::std_sink(color);
    }
  }

  pub fn err_supports_color(&self) -> bool {
    match &self.sink {
      Sink::Std { err, .. } => err.current_choice() != anstream::ColorChoice::Never,
      Sink::Write(_) => false,
    }
  }

  pub fn is_err_tty(&self) -> bool {
    match &self.sink {
      Sink::Std { .. } => io::IsTerminal::is_terminal(&io::stderr()),
      Sink::Write(_) => false,
    }
  }

  pub fn out(&mut self) -> &mut dyn Write {
    match &mut self.sink {
      Sink::Std { out, .. } => out,
      Sink::Write(w) => w,
    }
  }

  pub fn err(&mut self) -> &mut dyn Write {
    match &mut self.sink {
      Sink::Std { err, .. } => err,
      Sink::Write(w) => w,
    }
  }

  pub fn status(
    &mut self,
    status: impl fmt::Display,
    message: impl fmt::Display,
  ) -> io::Result<()> {
    self.status_with_style(status, message, &style::HEADER)
  }

  pub fn status_with_style(
    &mut self,
    status: impl fmt::Display,
    message: impl fmt::Display,
    style: &Style,
  ) -> io::Result<()> {
    if self.verbosity == Verbosity::Quiet {
      return Ok(());
    }
    let status = status.to_string();
    self.write_err(&format!(
      "{style}{status:>STATUS_WIDTH$}{style:#} {message}\n"
    ))
  }

  pub fn verbose(&mut self, f: impl FnOnce(&mut Self) -> io::Result<()>) -> io::Result<()> {
    if self.verbosity == Verbosity::Verbose {
      f(self)
    } else {
      Ok(())
    }
  }

  pub fn error(&mut self, message: impl fmt::Display) -> io::Result<()> {
    self.labeled("error", message, &style::ERROR)
  }

  pub fn warn(&mut self, message: impl fmt::Display) -> io::Result<()> {
    if self.verbosity == Verbosity::Quiet {
      return Ok(());
    }
    self.labeled("warning", message, &style::WARN)
  }

  pub fn note(&mut self, message: impl fmt::Display) -> io::Result<()> {
    if self.verbosity == Verbosity::Quiet {
      return Ok(());
    }
    self.labeled("note", message, &style::NOTE)
  }

  pub fn finished(&mut self, what: impl fmt::Display, elapsed: Duration) -> io::Result<()> {
    self.status(
      "Finished",
      format_args!("{what} in {}", format_duration(elapsed)),
    )
  }

  pub fn progress(&mut self, status: &str, len: u64) -> ProgressBar {
    let target = if self.verbosity == Verbosity::Quiet || !self.is_err_tty() {
      ProgressDrawTarget::hidden()
    } else {
      ProgressDrawTarget::stderr()
    };
    let template = if self.err_supports_color() {
      "{prefix:>12.cyan.bold} [{bar:27}] {pos}/{len}{wide_msg}"
    } else {
      "{prefix:>12} [{bar:27}] {pos}/{len}{wide_msg}"
    };
    let style = ProgressStyle::with_template(template)
      .expect("progress template is valid")
      .progress_chars("=> ");
    let bar = ProgressBar::with_draw_target(Some(len), target)
      .with_style(style)
      .with_prefix(status.to_owned());
    self.progress = Some(bar.clone());
    bar
  }

  fn labeled(&mut self, label: &str, message: impl fmt::Display, style: &Style) -> io::Result<()> {
    self.write_err(&format!("{style}{label}{style:#}: {message}\n"))
  }

  fn write_err(&mut self, text: &str) -> io::Result<()> {
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

pub fn format_duration(elapsed: Duration) -> String {
  let secs = elapsed.as_secs();
  if secs >= 60 {
    format!("{}m {:02}s", secs / 60, secs % 60)
  } else {
    format!("{:.2}s", elapsed.as_secs_f64())
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, rc::Rc};

  use super::*;

  #[derive(Clone, Default)]
  struct Capture(Rc<RefCell<Vec<u8>>>);

  impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
      self.0.borrow_mut().extend_from_slice(buf);
      Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
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
  fn durations() {
    assert_eq!(format_duration(Duration::from_millis(1234)), "1.23s");
    assert_eq!(format_duration(Duration::from_secs(125)), "2m 05s");
  }

  #[test]
  fn color_choice_parses() {
    assert_eq!("never".parse::<ColorChoice>(), Ok(ColorChoice::Never));
    assert!("sometimes".parse::<ColorChoice>().is_err());
  }
}
