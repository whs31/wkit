mod cli;
mod commands;
mod context;
mod failure;

use self::{cli::Cli, failure::Failure};

fn main() {
  let cli = <Cli as clap::Parser>::parse();
  let mut shell = cli.shell();
  if let Err(error) = cli.run(&mut shell) {
    std::process::exit(report(&mut shell, &error));
  }
}

fn report(shell: &mut wkit_core::Shell, error: &anyhow::Error) -> i32 {
  if let Some(Failure {
    message: None,
    code,
  }) = error.downcast_ref::<Failure>()
  {
    return *code;
  }
  let _ = shell.error(error);
  let mut causes = error.chain().skip(1).peekable();
  if causes.peek().is_some() {
    let err = shell.err();
    let _ = writeln!(err, "\nCaused by:");
    for cause in causes {
      for line in cause.to_string().lines() {
        let _ = writeln!(err, "  {line}");
      }
    }
  }
  Failure::exit_code(error)
}
