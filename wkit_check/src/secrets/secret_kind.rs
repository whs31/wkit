#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretKind {
  pub name: &'static str,
  pub pattern: &'static str,
}

impl SecretKind {
  pub const ALL: &[Self] = &[
    Self {
      name: "GitHub token",
      pattern: r"\b(?:github_pat_[A-Za-z0-9_]{22,}|gh[pousr]_[A-Za-z0-9]{36,})",
    },
    Self {
      name: "GitLab token",
      pattern: r"\bglpat-[A-Za-z0-9_-]{20,}",
    },
    Self {
      name: "AWS access key",
      pattern: r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
    },
    Self {
      name: "private key",
      pattern: r"-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----",
    },
    Self {
      name: "Slack token",
      pattern: r"\bxox[abprs]-[A-Za-z0-9-]{10,}",
    },
    Self {
      name: "Anthropic API key",
      pattern: r"\bsk-ant-[A-Za-z0-9_-]{20,}",
    },
    Self {
      name: "OpenAI API key",
      pattern: r"\bsk-proj-[A-Za-z0-9_-]{20,}",
    },
    Self {
      name: "Google API key",
      pattern: r"\bAIza[0-9A-Za-z_-]{35}\b",
    },
  ];
}
