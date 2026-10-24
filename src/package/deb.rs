use std::{
  fmt::Write as _,
  io::Write as _,
  path::{Path, PathBuf},
};

use super::{Packaging, Stage};
use crate::{
  Result, Shell, hint,
  manifest::DebPackage,
  version::{Flavor, ProjectVersion},
};

pub fn deb(packaging: &Packaging<'_>, shell: &mut Shell, extra: &[String]) -> Result<PathBuf> {
  if packaging.is_cargo_project() {
    cargo_deb(packaging, shell, extra)
  } else {
    anyhow::ensure!(extra.is_empty(), "extra arguments only apply to cargo-deb");
    native(packaging, shell)
  }
}

fn native(packaging: &Packaging<'_>, shell: &mut Shell) -> Result<PathBuf> {
  let project = packaging.project;
  let config = project.manifest.package.deb.clone().unwrap_or_default();
  let mut control = Control::new(
    &packaging.name,
    &packaging.version()?,
    &config,
    project.manifest.project.description.as_deref(),
  )?;
  let build_dir = packaging.build_native(shell)?;
  shell.status(
    "Packaging",
    format_args!("{} v{} (deb)", control.name, control.version),
  )?;
  let stage = Stage::prepare(packaging, "deb")?;
  stage.install_tree(shell, &build_dir, &config.prefix)?;
  stage.add_files()?;
  stage.add_license(
    &Path::new("/usr/share/doc")
      .join(&packaging.name)
      .join("copyright"),
  )?;
  stage.run_hook(shell, Some(&build_dir))?;
  control.installed_size = installed_size(&stage.dir)?;
  let output = packaging.output.join(control.file_name());
  write_archive(
    &stage.dir,
    &project.join(&config.scripts),
    &control.render(),
    &output,
  )?;
  shell.status("Packaged", packaging.shown(&output))?;
  Ok(output)
}

fn cargo_deb(packaging: &Packaging<'_>, shell: &mut Shell, extra: &[String]) -> Result<PathBuf> {
  if which::which("cargo-deb").is_err() {
    return Err(hint(
      "Debian packages of cargo projects need cargo-deb",
      "install it with `cargo install cargo-deb`",
    ));
  }
  let project = packaging.project;
  let version = packaging.version()?;
  let output = packaging.output.join(format!(
    "{}_{}_{}.deb",
    packaging.name,
    version.render(Flavor::Debian),
    host_architecture()
  ));
  crate::build::Builder::new(project, &packaging.options)
    .cargo_build(packaging.build_type(), false)?;
  shell.status(
    "Packaging",
    format_args!("{} v{version} (cargo-deb)", packaging.name),
  )?;
  fs_err::create_dir_all(&packaging.output)?;
  packaging.run_tool(
    shell,
    &cargo_deb_command(packaging, &output, extra),
    "Packaging",
    |_| None,
  )?;
  shell.status("Packaged", packaging.shown(&output))?;
  Ok(output)
}

fn cargo_deb_command(
  packaging: &Packaging<'_>,
  output: &Path,
  extra: &[String],
) -> crate::ProcessBuilder {
  let project = packaging.project;
  let config = &project.manifest.build.cargo;
  let mut cargo = config.command(&project.root, &["deb", "--no-build"]);
  if let Some(package) = &config.package {
    cargo = cargo.args(["--package", package]);
  }
  cargo.arg("--output").arg(output).args(extra)
}

pub fn host_architecture() -> &'static str {
  match std::env::consts::ARCH {
    "x86_64" => "amd64",
    "aarch64" => "arm64",
    "arm" => "armhf",
    "x86" => "i386",
    "powerpc64" => "ppc64el",
    other => other,
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Control {
  name: String,
  version: String,
  architecture: String,
  maintainer: String,
  installed_size: u64,
  description: String,
  fields: Vec<(&'static str, String)>,
}

impl Control {
  fn new(
    name: &str,
    version: &ProjectVersion,
    config: &DebPackage,
    fallback_description: Option<&str>,
  ) -> Result<Self> {
    let maintainer = config.maintainer.clone().ok_or_else(|| {
      hint(
        "Debian packages need a maintainer",
        "set `package.deb.maintainer = \"Name <email>\"` in wkit.toml",
      )
    })?;
    let description = config
      .description
      .as_deref()
      .or(fallback_description)
      .filter(|d| !d.trim().is_empty())
      .ok_or_else(|| {
        hint(
          "Debian packages need a description",
          "set `project.description` or `package.deb.description`",
        )
      })?;
    let mut fields = Vec::new();
    if let Some(section) = &config.section {
      fields.push(("Section", section.clone()));
    }
    fields.push(("Priority", config.priority.clone()));
    if let Some(homepage) = &config.homepage {
      fields.push(("Homepage", homepage.clone()));
    }
    fields.extend(
      config
        .relations()
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(f, v)| (f, v.join(", "))),
    );
    Ok(Self {
      name: name.to_owned(),
      version: version.render(Flavor::Debian),
      architecture: config
        .architecture
        .clone()
        .unwrap_or_else(|| host_architecture().to_owned()),
      maintainer,
      installed_size: 0,
      description: description.trim().to_owned(),
      fields,
    })
  }

  fn file_name(&self) -> String {
    format!("{}_{}_{}.deb", self.name, self.version, self.architecture)
  }

  fn render(&self) -> String {
    let mut text = format!(
      "Package: {}\nVersion: {}\nArchitecture: {}\nMaintainer: {}\nInstalled-Size: {}\n",
      self.name, self.version, self.architecture, self.maintainer, self.installed_size
    );
    for (field, value) in &self.fields {
      let _ = writeln!(text, "{field}: {value}");
    }
    let mut lines = self.description.lines();
    let _ = writeln!(text, "Description: {}", lines.next().unwrap_or_default());
    for line in lines.map(str::trim_end) {
      let _ = writeln!(text, " {}", if line.is_empty() { "." } else { line });
    }
    text
  }
}

type Tar = tar::Builder<flate2::write::GzEncoder<Vec<u8>>>;

const SCRIPTS: &[&str] = &["preinst", "postinst", "prerm", "postrm", "triggers"];

fn installed_size(root: &Path) -> Result<u64> {
  Ok(
    entries(root)?
      .iter()
      .map(|(_, _, meta)| {
        if meta.is_file() {
          meta.len().div_ceil(1024)
        } else {
          1
        }
      })
      .sum(),
  )
}

fn write_archive(root: &Path, scripts: &Path, control: &str, output: &Path) -> Result<()> {
  let entries = entries(root)?;
  let conffiles: String = entries
    .iter()
    .filter(|(name, _, meta)| meta.is_file() && name.starts_with("etc/"))
    .flat_map(|(name, _, _)| ["/", name.as_str(), "\n"])
    .collect();

  let mut control_tar = tar();
  append_bytes(
    &mut control_tar,
    "./",
    &[],
    0o755,
    tar::EntryType::Directory,
  )?;
  append_bytes(
    &mut control_tar,
    "./control",
    control.as_bytes(),
    0o644,
    tar::EntryType::Regular,
  )?;
  if !conffiles.is_empty() {
    append_bytes(
      &mut control_tar,
      "./conffiles",
      conffiles.as_bytes(),
      0o644,
      tar::EntryType::Regular,
    )?;
  }
  for name in SCRIPTS {
    let path = scripts.join(name);
    if path.is_file() {
      let mode = if *name == "triggers" { 0o644 } else { 0o755 };
      append_bytes(
        &mut control_tar,
        &format!("./{name}"),
        &fs_err::read(&path)?,
        mode,
        tar::EntryType::Regular,
      )?;
    }
  }

  let mut data_tar = tar();
  append_bytes(&mut data_tar, "./", &[], 0o755, tar::EntryType::Directory)?;
  for (name, path, meta) in &entries {
    let mut header = tar::Header::new_gnu();
    header.set_metadata_in_mode(meta, tar::HeaderMode::Complete);
    own_by_root(&mut header);
    let name = format!("./{name}");
    if meta.is_symlink() {
      header.set_entry_type(tar::EntryType::Symlink);
      header.set_size(0);
      data_tar.append_link(&mut header, &name, fs_err::read_link(path)?)?;
    } else if meta.is_dir() {
      header.set_entry_type(tar::EntryType::Directory);
      header.set_size(0);
      data_tar.append_data(&mut header, format!("{name}/"), std::io::empty())?;
    } else {
      data_tar.append_data(&mut header, &name, fs_err::File::open(path)?)?;
    }
  }

  if let Some(parent) = output.parent() {
    fs_err::create_dir_all(parent)?;
  }
  let mut out = std::io::BufWriter::new(fs_err::File::create(output)?);
  out.write_all(b"!<arch>\n")?;
  for (name, data) in [
    ("debian-binary", b"2.0\n".to_vec()),
    ("control.tar.gz", control_tar.into_inner()?.finish()?),
    ("data.tar.gz", data_tar.into_inner()?.finish()?),
  ] {
    writeln!(
      out,
      "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`",
      mtime(),
      0,
      0,
      "100644",
      data.len()
    )?;
    out.write_all(&data)?;
    if data.len() % 2 == 1 {
      out.write_all(b"\n")?;
    }
  }
  out.flush()?;
  Ok(())
}

fn entries(root: &Path) -> Result<Vec<(String, PathBuf, std::fs::Metadata)>> {
  let mut found = Vec::new();
  let mut pending = vec![root.to_owned()];
  while let Some(dir) = pending.pop() {
    for entry in fs_err::read_dir(&dir)? {
      let path = entry?.path();
      let meta = fs_err::symlink_metadata(&path)?;
      if meta.is_dir() {
        pending.push(path.clone());
      }
      let relative = path.strip_prefix(root).unwrap_or(&path);
      let name = relative
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

fn tar() -> Tar {
  let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
    Vec::new(),
    flate2::Compression::default(),
  ));
  tar.follow_symlinks(false);
  tar
}

fn own_by_root(header: &mut tar::Header) {
  header.set_uid(0);
  header.set_gid(0);
  header.set_mtime(mtime());
  let _ = header.set_username("root");
  let _ = header.set_groupname("root");
}

fn append_bytes(
  tar: &mut Tar,
  name: &str,
  data: &[u8],
  mode: u32,
  kind: tar::EntryType,
) -> Result<()> {
  let mut header = tar::Header::new_gnu();
  own_by_root(&mut header);
  header.set_entry_type(kind);
  header.set_mode(mode);
  header.set_size(data.len() as u64);
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::version::Scheme;

  #[test]
  fn renders_relations_and_folded_descriptions() {
    let config = DebPackage {
      maintainer: Some("whs31 <whs31@github.io>".into()),
      section: Some("utils".into()),
      architecture: Some("amd64".into()),
      depends: vec!["libc6 (>= 2.28)".into(), "mmsc".into()],
      breaks: vec!["gcs2".into()],
      ..DebPackage::default()
    };
    let version = ProjectVersion::parse("3.2.0-rc.1", Scheme::SemVer).unwrap();
    let mut control = Control::new(
      "ics2",
      &version,
      &config,
      Some("Ground control\nRuns the field.\n\nKeeps logs."),
    )
    .unwrap();
    control.installed_size = 42;
    assert_eq!(control.file_name(), "ics2_3.2.0~rc.1_amd64.deb");
    assert_eq!(
      control.render(),
      "Package: ics2\nVersion: 3.2.0~rc.1\nArchitecture: amd64\nMaintainer: whs31 <whs31@github.io>\nInstalled-Size: 42\nSection: utils\nPriority: optional\nDepends: libc6 (>= 2.28), mmsc\nBreaks: gcs2\nDescription: Ground control\n Runs the field.\n .\n Keeps logs.\n"
    );
  }

  #[test]
  fn maintainer_and_description_are_required() {
    let version = ProjectVersion::parse("1.0.0", Scheme::SemVer).unwrap();
    let err = Control::new("x", &version, &DebPackage::default(), Some("d")).unwrap_err();
    assert!(err.to_string().contains("package.deb.maintainer"), "{err}");
    let config = DebPackage {
      maintainer: Some("m".into()),
      ..DebPackage::default()
    };
    let err = Control::new("x", &version, &config, None).unwrap_err();
    assert!(err.to_string().contains("project.description"), "{err}");
  }

  #[cfg(unix)]
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

    assert_eq!(installed_size(&root).unwrap(), 7);
    let out = dir.path().join("out/x.deb");
    write_archive(&root, &scripts, "Package: x\n", &out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"!<arch>\ndebian-binary   "));
    assert_eq!(
      names(member(&bytes, "control.tar.gz")),
      ["./", "control", "conffiles", "postinst"]
    );
    assert_eq!(
      names(member(&bytes, "data.tar.gz")),
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
