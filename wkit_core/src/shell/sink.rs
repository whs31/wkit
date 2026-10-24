pub(super) enum Sink {
  Std {
    out: anstream::AutoStream<std::io::Stdout>,
    err: anstream::AutoStream<std::io::Stderr>,
  },
  Write(anstream::AutoStream<Box<dyn std::io::Write>>),
}

impl Sink {
  pub(super) fn std(color: super::ColorChoice) -> Self {
    Self::Std {
      out: anstream::AutoStream::new(std::io::stdout(), color.to_stream()),
      err: anstream::AutoStream::new(std::io::stderr(), color.to_stream()),
    }
  }

  pub(super) fn write(out: Box<dyn std::io::Write>) -> Self {
    Self::Write(anstream::AutoStream::new(out, anstream::ColorChoice::Never))
  }

  pub(super) fn out(&mut self) -> &mut dyn std::io::Write {
    match self {
      Self::Std { out, .. } => out,
      Self::Write(w) => w,
    }
  }

  pub(super) fn err(&mut self) -> &mut dyn std::io::Write {
    match self {
      Self::Std { err, .. } => err,
      Self::Write(w) => w,
    }
  }

  pub(super) fn err_supports_color(&self) -> bool {
    match self {
      Self::Std { err, .. } => err.current_choice() != anstream::ColorChoice::Never,
      Self::Write(_) => false,
    }
  }

  pub(super) fn is_err_tty(&self) -> bool {
    match self {
      Self::Std { .. } => std::io::IsTerminal::is_terminal(&std::io::stderr()),
      Self::Write(_) => false,
    }
  }
}
