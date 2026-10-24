use super::index_entry::IndexEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateIndex {
  entries: Vec<IndexEntry>,
}

impl CrateIndex {
  pub const URL: &str = "https://index.crates.io/wk/it/wkit";

  pub fn fetch() -> anyhow::Result<Self> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
      .timeout_global(Some(std::time::Duration::from_secs(30)))
      .user_agent(concat!(
        "wkit/",
        env!("CARGO_PKG_VERSION"),
        " (+",
        env!("CARGO_PKG_REPOSITORY"),
        ")"
      ))
      .build()
      .into();
    let context = || format!("failed to fetch `{}`", Self::URL);
    let mut response = match agent.get(Self::URL).call() {
      Ok(response) => response,
      Err(ureq::Error::StatusCode(404)) => anyhow::bail!("wkit is not published on crates.io"),
      Err(e) => return Err(anyhow::Error::from(e).context(context())),
    };
    let text = response
      .body_mut()
      .read_to_string()
      .map_err(|e| anyhow::Error::from(e).context(context()))?;
    Self::parse(&text)
  }

  pub fn parse(text: &str) -> anyhow::Result<Self> {
    let entries = text
      .lines()
      .filter(|line| !line.trim().is_empty())
      .map(serde_json::from_str)
      .collect::<Result<_, _>>()
      .map_err(|e| anyhow::Error::from(e).context("crates.io returned a malformed index entry"))?;
    Ok(Self { entries })
  }

  pub fn select(
    &self,
    current: &semver::Version,
    requirement: Option<&semver::VersionReq>,
    pinned: Option<&semver::Version>,
  ) -> anyhow::Result<semver::Version> {
    if let Some(pinned) = pinned {
      return match self.entries.iter().find(|e| e.vers == *pinned) {
        Some(entry) if entry.yanked => anyhow::bail!("wkit {pinned} was yanked"),
        Some(entry) => Ok(entry.vers.clone()),
        None => anyhow::bail!("wkit {pinned} is not published on crates.io"),
      };
    }
    self
      .entries
      .iter()
      .filter(|e| !e.yanked)
      .filter(|e| e.vers.pre.is_empty() || !current.pre.is_empty())
      .filter(|e| requirement.is_none_or(|r| r.matches(&e.vers)))
      .map(|e| &e.vers)
      .max()
      .cloned()
      .ok_or_else(|| match requirement {
        Some(r) => anyhow::anyhow!("no published wkit version matches `{r}` from `project.wkit`"),
        None => anyhow::anyhow!("no published wkit versions found"),
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn index() -> CrateIndex {
    CrateIndex::parse(concat!(
      r#"{"name":"wkit","vers":"0.1.0","deps":[],"cksum":"x","features":{},"yanked":false}"#,
      "\n",
      r#"{"name":"wkit","vers":"0.2.0","deps":[],"cksum":"x","features":{},"yanked":false}"#,
      "\n",
      r#"{"name":"wkit","vers":"0.3.0-rc.1","deps":[],"cksum":"x","features":{},"yanked":false}"#,
      "\n",
      r#"{"name":"wkit","vers":"0.2.1","deps":[],"cksum":"x","features":{},"yanked":true}"#,
      "\n",
    ))
    .unwrap()
  }

  fn v(s: &str) -> semver::Version {
    semver::Version::parse(s).unwrap()
  }

  #[test]
  fn picks_the_newest_stable_unyanked_release() {
    assert_eq!(index().select(&v("0.1.0"), None, None).unwrap(), v("0.2.0"));
    assert_eq!(
      index().select(&v("0.3.0-alpha.1"), None, None).unwrap(),
      v("0.3.0-rc.1")
    );
  }

  #[test]
  fn honors_requirements_and_pins() {
    let req = semver::VersionReq::parse("~0.1").unwrap();
    assert_eq!(
      index().select(&v("0.1.0"), Some(&req), None).unwrap(),
      v("0.1.0")
    );
    let req = semver::VersionReq::parse(">=1").unwrap();
    assert!(index().select(&v("0.1.0"), Some(&req), None).is_err());
    assert_eq!(
      index()
        .select(&v("0.2.0"), None, Some(&v("0.1.0")))
        .unwrap(),
      v("0.1.0")
    );
    assert!(
      index()
        .select(&v("0.2.0"), None, Some(&v("0.2.1")))
        .is_err()
    );
    assert!(
      index()
        .select(&v("0.2.0"), None, Some(&v("9.9.9")))
        .is_err()
    );
  }
}
