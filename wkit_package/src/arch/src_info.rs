#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SrcInfo {
  pub pkgver: String,
  pub pkgnames: Vec<String>,
  pub sources: Vec<String>,
  pub depends: Vec<String>,
}

impl SrcInfo {
  pub const ARCHIVE_SUFFIXES: &[&str] = &[".tar.gz", ".tar.xz", ".tar.zst", ".tgz"];

  pub fn parse(text: &str) -> Self {
    let mut info = Self::default();
    let mut in_base = true;
    for line in text.lines() {
      let Some((key, value)) = line.split_once(" = ") else {
        continue;
      };
      let value = value.trim().to_owned();
      match key.trim() {
        "pkgname" => {
          in_base = false;
          info.pkgnames.push(value);
        }
        "pkgver" => info.pkgver = value,
        "source" if in_base => info.sources.push(value),
        "depends" | "makedepends" if in_base => info.depends.push(value),
        _ => {}
      }
    }
    info
  }

  pub fn local_archives(&self) -> impl Iterator<Item = (&str, &str)> {
    self.sources.iter().filter_map(|source| {
      let file = source
        .split_once("::")
        .map_or(source.as_str(), |(name, _)| name);
      if source.contains("://") {
        return None;
      }
      Self::ARCHIVE_SUFFIXES
        .iter()
        .find_map(|suffix| file.strip_suffix(suffix))
        .map(|prefix| (file, prefix))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_versions_names_and_local_archives() {
    let info = SrcInfo::parse(
      "pkgbase = mmsc\n\tpkgdesc = MMS\n\tpkgver = 3.12.0rc.1\n\tpkgrel = 1\n\tmakedepends = cmake\n\tdepends = glibc\n\tsource = mms-3.12.0-rc.1.tar.gz\n\tsource = https://example.com/x.tar.gz\n\tsource = mmsc.install\n\npkgname = mmsc\n\tdepends = qt6-base\n",
    );
    assert_eq!(info.pkgver, "3.12.0rc.1");
    assert_eq!(info.pkgnames, ["mmsc"]);
    assert_eq!(info.depends, ["cmake", "glibc"]);
    assert_eq!(
      info.local_archives().collect::<Vec<_>>(),
      [("mms-3.12.0-rc.1.tar.gz", "mms-3.12.0-rc.1")]
    );
  }
}
