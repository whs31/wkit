mod check;
mod check_options;
mod checker;
mod clang_tidy;
mod clippy;
mod custom;
mod digest;
mod error;
mod format_check;
mod outcome;
mod pre_commit;
mod secrets;
mod stage_report;
mod status;
mod versions_check;
mod whitespace;

pub use self::{
  check::Check, check_options::CheckOptions, checker::Checker, clang_tidy::ClangTidy,
  clippy::Clippy, custom::Custom, digest::Digest, error::Error, format_check::FormatCheck,
  outcome::Outcome, pre_commit::PreCommitPins, secrets::Secrets, stage_report::StageReport,
  status::Status, versions_check::VersionsCheck, whitespace::Whitespace,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
