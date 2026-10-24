#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Distro {
  #[display("Debian-like")]
  Debian,

  #[display("Arch-like")]
  Arch,

  #[display("{_0}")]
  Other(String),
}

impl Distro {
  pub const OS_RELEASE: &[&str] = &["/etc/os-release", "/usr/lib/os-release"];
  const DEBIAN: &[&str] = &["debian", "ubuntu", "linuxmint", "astra"];
  const ARCH: &[&str] = &["arch", "manjaro", "endeavouros"];

  pub fn detect() -> Self {
    Self::OS_RELEASE
      .iter()
      .find_map(|path| std::fs::read_to_string(path).ok())
      .map_or_else(
        || Self::Other(std::env::consts::OS.to_owned()),
        |text| Self::parse(&text),
      )
  }

  pub fn parse(os_release: &str) -> Self {
    let field = |key: &str| {
      os_release
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().trim_matches(['"', '\'']).to_owned())
        .unwrap_or_default()
    };
    let id = field("ID");
    let like = field("ID_LIKE");
    let is = |known: &[&str]| {
      known.contains(&id.as_str()) || like.split_whitespace().any(|l| known.contains(&l))
    };
    if is(Self::DEBIAN) {
      Self::Debian
    } else if is(Self::ARCH) {
      Self::Arch
    } else {
      let pretty = field("PRETTY_NAME");
      Self::Other(if pretty.is_empty() { id } else { pretty })
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_id_and_id_like() {
    assert_eq!(Distro::parse("ID=arch\n"), Distro::Arch);
    assert_eq!(
      Distro::parse("ID=pop\nID_LIKE=\"ubuntu debian\"\n"),
      Distro::Debian
    );
    assert_eq!(Distro::parse("ID=cachyos\nID_LIKE=arch\n"), Distro::Arch);
    assert_eq!(
      Distro::parse("ID=fedora\nPRETTY_NAME=\"Fedora Linux 43\"\n"),
      Distro::Other("Fedora Linux 43".into())
    );
  }
}
