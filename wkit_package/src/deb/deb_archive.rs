use std::io::Write;

use crate::Result;

#[derive(Debug, Clone, Default)]
pub struct DebArchive {
  scripts: Vec<(String, std::path::PathBuf)>,
}

impl DebArchive {
  pub const SCRIPTS: &[&str] = &["preinst", "postinst", "prerm", "postrm", "triggers"];

  pub fn with_scripts(dir: &std::path::Path) -> Self {
    let scripts = Self::SCRIPTS
      .iter()
      .map(|&name| (name.to_owned(), dir.join(name)))
      .filter(|(_, path)| path.is_file())
      .collect();
    Self { scripts }
  }

  pub fn installed_size(root: &std::path::Path) -> Result<u64> {
    let mut total = 0;
    for (_, path, meta) in Self::entries(root)? {
      let _ = path;
      total += if meta.is_file() {
        meta.len().div_ceil(1024)
      } else {
        1
      };
    }
    Ok(total)
  }

  pub fn write(
    &self,
    root: &std::path::Path,
    control: &str,
    output: &std::path::Path,
  ) -> Result<()> {
    let conffiles: Vec<String> = Self::entries(root)?
      .into_iter()
      .filter(|(name, _, meta)| meta.is_file() && name.starts_with("etc/"))
      .map(|(name, _, _)| format!("/{name}"))
      .collect();
    let mut conffiles = conffiles.join("\n");
    if !conffiles.is_empty() {
      conffiles.push('\n');
    }
    let control_tar = self.control_tar(control, &conffiles)?;
    let data_tar = Self::data_tar(root)?;
    if let Some(parent) = output.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let mut out = std::io::BufWriter::new(std::fs::File::create(output).map_err(|source| {
      wkit_core::Error::Write {
        path: output.to_owned(),
        source,
      }
    })?);
    out.write_all(b"!<arch>\n")?;
    for (name, data) in [
      ("debian-binary", &b"2.0\n"[..]),
      ("control.tar.gz", &control_tar),
      ("data.tar.gz", &data_tar),
    ] {
      writeln!(
        out,
        "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`",
        Self::mtime(),
        0,
        0,
        "100644",
        data.len()
      )?;
      out.write_all(data)?;
      if data.len() % 2 == 1 {
        out.write_all(b"\n")?;
      }
    }
    out.flush()?;
    Ok(())
  }

  fn control_tar(&self, control: &str, conffiles: &str) -> Result<Vec<u8>> {
    let mut tar = Self::tar();
    Self::append_dir(&mut tar, "./")?;
    Self::append_bytes(&mut tar, "./control", control.as_bytes(), 0o644)?;
    if !conffiles.is_empty() {
      Self::append_bytes(&mut tar, "./conffiles", conffiles.as_bytes(), 0o644)?;
    }
    for (name, path) in &self.scripts {
      let mode = if name == "triggers" { 0o644 } else { 0o755 };
      Self::append_bytes(&mut tar, &format!("./{name}"), &std::fs::read(path)?, mode)?;
    }
    Ok(tar.into_inner()?.finish()?)
  }

  fn data_tar(root: &std::path::Path) -> Result<Vec<u8>> {
    let mut tar = Self::tar();
    Self::append_dir(&mut tar, "./")?;
    for (name, path, meta) in Self::entries(root)? {
      let mut header = Self::header(&meta);
      let name = format!("./{name}");
      if meta.is_symlink() {
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        tar.append_link(&mut header, &name, std::fs::read_link(&path)?)?;
      } else if meta.is_dir() {
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        tar.append_data(&mut header, format!("{name}/"), std::io::empty())?;
      } else {
        tar.append_data(&mut header, &name, std::fs::File::open(&path)?)?;
      }
    }
    Ok(tar.into_inner()?.finish()?)
  }

  fn entries(
    root: &std::path::Path,
  ) -> Result<Vec<(String, std::path::PathBuf, std::fs::Metadata)>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
      for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.is_dir() {
          pending.push(path.clone());
        }
        let name = path
          .strip_prefix(root)
          .unwrap_or(&path)
          .components()
          .map(|c| c.as_os_str().to_string_lossy())
          .collect::<Vec<_>>()
          .join("/");
        found.push((name, path, meta));
      }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
  }

  fn tar() -> tar::Builder<flate2::write::GzEncoder<Vec<u8>>> {
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
      Vec::new(),
      flate2::Compression::default(),
    ));
    tar.follow_symlinks(false);
    tar
  }

  fn header(meta: &std::fs::Metadata) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_metadata_in_mode(meta, tar::HeaderMode::Complete);
    Self::own_by_root(&mut header);
    header
  }

  fn root_header() -> tar::Header {
    let mut header = tar::Header::new_gnu();
    Self::own_by_root(&mut header);
    header
  }

  fn own_by_root(header: &mut tar::Header) {
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(Self::mtime());
    let _ = header.set_username("root");
    let _ = header.set_groupname("root");
  }

  fn append_dir(
    tar: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
    name: &str,
  ) -> Result<()> {
    let mut header = Self::root_header();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_mode(0o755);
    header.set_size(0);
    header.set_mtime(Self::mtime());
    tar.append_data(&mut header, name, std::io::empty())?;
    Ok(())
  }

  fn append_bytes(
    tar: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
    name: &str,
    data: &[u8],
    mode: u32,
  ) -> Result<()> {
    let mut header = Self::root_header();
    header.set_mode(mode);
    header.set_size(data.len() as u64);
    header.set_mtime(Self::mtime());
    tar.append_data(&mut header, name, data)?;
    Ok(())
  }

  fn mtime() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or_else(|| {
        std::time::SystemTime::now()
          .duration_since(std::time::UNIX_EPOCH)
          .map_or(0, |d| d.as_secs())
      })
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;

  #[test]
  fn writes_an_ar_archive_with_control_and_data() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(root.join("usr/bin")).unwrap();
    std::fs::create_dir_all(root.join("etc/x")).unwrap();
    std::fs::write(root.join("usr/bin/x"), "#!/bin/sh\n").unwrap();
    std::fs::write(root.join("etc/x/x.toml"), "a = 1\n").unwrap();
    std::os::unix::fs::symlink("x", root.join("usr/bin/y")).unwrap();
    let scripts = dir.path().join("debian");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join("postinst"), "#!/bin/sh\n").unwrap();

    assert_eq!(DebArchive::installed_size(&root).unwrap(), 7);
    let out = dir.path().join("out/x.deb");
    DebArchive::with_scripts(&scripts)
      .write(&root, "Package: x\n", &out)
      .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"!<arch>\ndebian-binary   "));
    let control = member(&bytes, "control.tar.gz");
    let control = names(control);
    assert_eq!(control, ["./", "control", "conffiles", "postinst"]);
    let data = names(member(&bytes, "data.tar.gz"));
    assert_eq!(
      data,
      [
        "./",
        "etc/",
        "etc/x/",
        "etc/x/x.toml",
        "usr/",
        "usr/bin/",
        "usr/bin/x",
        "usr/bin/y"
      ]
    );
    if which::which("dpkg-deb").is_ok() {
      let info = std::process::Command::new("dpkg-deb")
        .arg("--contents")
        .arg(&out)
        .output()
        .unwrap();
      let listing = String::from_utf8_lossy(&info.stdout);
      assert!(
        info.status.success(),
        "{}",
        String::from_utf8_lossy(&info.stderr)
      );
      assert!(
        listing.contains("root/root") && listing.contains("usr/bin/y -> x"),
        "{listing}"
      );
    }
  }

  fn member<'a>(bytes: &'a [u8], name: &str) -> &'a [u8] {
    let mut offset = 8;
    while offset < bytes.len() {
      let header = std::str::from_utf8(&bytes[offset..offset + 60]).unwrap();
      let size: usize = header[48..58].trim().parse().unwrap();
      let start = offset + 60;
      if header[..16].trim() == name {
        return &bytes[start..start + size];
      }
      offset = start + size + size % 2;
    }
    panic!("no member {name}");
  }

  fn names(tar_gz: &[u8]) -> Vec<String> {
    tar::Archive::new(flate2::read::GzDecoder::new(tar_gz))
      .entries()
      .unwrap()
      .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
      .collect()
  }
}
