mod committer;
mod error;
mod fmt_target;
mod git_run;
mod member;
mod member_status;
mod member_sync;
mod phase;
mod repo;
mod report;
mod runner;
mod task_outcome;
mod task_status;
mod workspace;
mod workspace_lock;
mod workspace_manifest;

pub use self::{
  committer::Committer, error::Error, fmt_target::FmtTarget, git_run::GitRun, member::Member,
  member_status::MemberStatus, member_sync::MemberSync, phase::Phase, repo::Repo, report::Report,
  runner::Runner, task_outcome::TaskOutcome, task_status::TaskStatus, workspace::Workspace,
  workspace_lock::WorkspaceLock, workspace_manifest::WorkspaceManifest,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
