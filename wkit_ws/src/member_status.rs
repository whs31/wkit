use crate::{Member, Repo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberStatus {
  pub path: String,
  pub head: Option<String>,
  pub branch: Option<String>,
  pub flags: Vec<String>,
  pub broken: bool,
}

impl MemberStatus {
  pub fn inspect(root: &std::path::Path, member: &Member, locked: Option<&str>) -> Self {
    let repo = Repo::new(member.dir(root));
    let mut status = Self {
      path: member.path.clone(),
      head: None,
      branch: None,
      flags: Vec::new(),
      broken: true,
    };
    if !repo.exists() {
      status.flags.push("missing".to_owned());
      return status;
    }
    if !repo.is_repository() {
      status.flags.push("not a Git repository".to_owned());
      return status;
    }
    status.broken = !repo.is_standalone();
    if status.broken {
      status.flags.push("not standalone".to_owned());
    }
    status.head = repo.head();
    status.branch = repo.current_branch();
    if repo.is_dirty() {
      status.flags.push("dirty".to_owned());
    }
    if status.head.as_deref() != locked {
      status.flags.push("unlocked".to_owned());
    }
    if let Some((ahead, behind)) = repo.upstream().and_then(|u| repo.ahead_behind(&u)) {
      if ahead > 0 {
        status.flags.push(format!("ahead {ahead}"));
      }
      if behind > 0 {
        status.flags.push(format!("behind {behind}"));
      }
    }
    status
  }

  pub fn render(&self, branch_width: usize) -> String {
    let flags = if self.flags.is_empty() {
      String::new()
    } else {
      format!("  {}", self.flags.join(", "))
    };
    if self.head.is_none() && self.broken {
      return format!(
        "{:<8}  {:<branch_width$}  {}{flags}",
        "--------", "", self.path
      );
    }
    format!(
      "{:<8}  {:<branch_width$}  {}{flags}",
      Repo::short(self.head.as_deref()),
      self.branch.as_deref().unwrap_or("detached"),
      self.path
    )
  }
}
