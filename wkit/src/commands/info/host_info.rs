use wkit_package::Distro;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HostInfo {
  pub triple: String,
  pub os: String,
  pub family: String,
  pub kernel: Option<String>,
  pub cpus: usize,
}

impl HostInfo {
  pub fn detect() -> Self {
    let os = Distro::OS_RELEASE
      .iter()
      .find_map(|path| std::fs::read_to_string(path).ok())
      .and_then(|text| Self::pretty_name(&text))
      .unwrap_or_else(|| std::env::consts::OS.to_owned());
    Self {
      triple: Self::triple(),
      os,
      family: Distro::detect().to_string(),
      kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|k| k.trim().to_owned()),
      cpus: std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    }
  }

  pub fn triple() -> String {
    let vendor = if cfg!(target_vendor = "apple") {
      "apple"
    } else if cfg!(target_vendor = "pc") {
      "pc"
    } else {
      "unknown"
    };
    let os = match std::env::consts::OS {
      "macos" => "darwin",
      other => other,
    };
    let env = if cfg!(target_env = "gnu") {
      "-gnu"
    } else if cfg!(target_env = "musl") {
      "-musl"
    } else if cfg!(target_env = "msvc") {
      "-msvc"
    } else {
      ""
    };
    format!("{}-{vendor}-{os}{env}", std::env::consts::ARCH)
  }

  pub fn pretty_name(os_release: &str) -> Option<String> {
    os_release
      .lines()
      .filter_map(|line| line.split_once('='))
      .find(|(key, _)| key.trim() == "PRETTY_NAME")
      .map(|(_, value)| value.trim().trim_matches(['"', '\'']).to_owned())
      .filter(|name| !name.is_empty())
  }

  pub fn fields(&self) -> Vec<(&'static str, String)> {
    let mut fields = vec![
      ("host", self.triple.clone()),
      ("os", format!("{} ({})", self.os, self.family)),
    ];
    if let Some(kernel) = &self.kernel {
      fields.push(("kernel", kernel.clone()));
    }
    fields.push(("cpus", self.cpus.to_string()));
    fields
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_triple_names_this_build_host() {
    let triple = HostInfo::triple();
    assert!(triple.starts_with(std::env::consts::ARCH), "{triple}");
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
      assert!(triple.ends_with("-unknown-linux-gnu"), "{triple}");
    }
  }

  #[test]
  fn pretty_names_come_from_os_release() {
    assert_eq!(
      HostInfo::pretty_name("NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n")
        .as_deref(),
      Some("Arch Linux")
    );
    assert_eq!(HostInfo::pretty_name("ID=arch\n"), None);
  }
}
