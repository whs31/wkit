mod arch;
mod deb;
mod install;

use std::{
  io::Write as _,
  path::{Path, PathBuf},
};

pub use self::{
  arch::arch,
  deb::deb,
  install::{install, uninstall},
};
use crate::{
  Failure, ProcessBuilder, Result, Shell,
  build::{BuildOptions, Builder, Conan},
  detect::BuildSystem,
  hint,
  manifest::{BuildType, FileMapping, FileMode, Project, WindowsGenerator},
  version::{Flavor, Mismatch, ProjectVersion, VersionSet},
};

#[derive(Debug, Clone)]
pub struct Packaging<'a> {
  pub project: &'a Project,
  pub options: BuildOptions,
  pub name: String,
  pub set: Option<String>,
  pub output: PathBuf,
  explicit_output: bool,
}

impl<'a> Packaging<'a> {
  const LOG: &'static str = "build/wkit-package.log";

  pub fn new(
    project: &'a Project,
    mut options: BuildOptions,
    set: Option<String>,
    output: Option<PathBuf>,
  ) -> Result<Self> {
    options.build_type.get_or_insert(BuildType::Release);
    let manifest = &project.manifest;
    let is_cargo = Self::is_cargo(project);
    let mut name = manifest
      .package
      .name
      .clone()
      .or_else(|| {
        is_cargo
          .then(|| manifest.build.cargo.package.clone())
          .flatten()
      })
      .unwrap_or_else(|| project.name());
    if let Some(set) = &set {
      let option = manifest.package.set_option.as_deref().ok_or_else(|| {
        hint(
          format!("cannot package the `{set}` set: no Conan option selects sets"),
          "set `package.set-option` in wkit.toml",
        )
      })?;
      anyhow::ensure!(
        Self::is_valid_set(set),
        "invalid set `{set}`; use lowercase letters, digits and `_`"
      );
      if Conan::new(project, &options)
        .default_option(option)?
        .as_deref()
        != Some(set)
      {
        name = format!("{name}-{}", set.replace('_', "-"));
      }
      options.conan_options.push(format!("{option}={set}"));
    }
    let output = output.or_else(|| manifest.package.output.clone());
    let explicit_output = output.is_some();
    let output = project.join(output.unwrap_or_else(|| {
      if is_cargo {
        "target/package"
      } else {
        "build/package"
      }
      .into()
    }));
    Ok(Self {
      project,
      options,
      name,
      set,
      output,
      explicit_output,
    })
  }

  pub fn build_type(&self) -> BuildType {
    self.options.build_type.unwrap_or(BuildType::Release)
  }

  pub fn is_cargo_project(&self) -> bool {
    Self::is_cargo(self.project)
  }

  pub fn version(&self) -> Result<ProjectVersion> {
    self.version_set()?.current()
  }

  fn mismatches(&self) -> Result<Vec<Mismatch>> {
    let set = self.version_set()?;
    Ok(set.mismatches(&set.current()?))
  }

  fn stage_dir(&self, format: &str) -> PathBuf {
    self
      .project
      .join("build/stage")
      .join(format)
      .join(&self.name)
  }

  fn shown(&self, path: &Path) -> String {
    path
      .strip_prefix(&self.project.root)
      .unwrap_or(path)
      .display()
      .to_string()
  }

  fn build_native(&self, shell: &mut Shell) -> Result<PathBuf> {
    Builder::new(self.project, &self.options)
      .run_native(shell)?
      .and_then(|outcome| outcome.build_dir().map(Path::to_owned))
      .ok_or_else(|| {
        hint(
          "nothing to package: the project has no CMake build",
          "native packages need conanfile.py or CMakeLists.txt with install() rules",
        )
      })
  }

  fn with_env(&self, process: ProcessBuilder) -> ProcessBuilder {
    let mut process = process
      .env("WKIT_PACKAGE_NAME", &self.name)
      .env("WKIT_BUILD_TYPE", self.build_type().to_string());
    if let Ok(version) = self.version() {
      process = process.env("WKIT_PACKAGE_VERSION", version.to_string());
    }
    match &self.set {
      Some(set) => process.env("WKIT_PACKAGE_SET", set),
      None => process,
    }
  }

  fn run_tool(
    &self,
    shell: &mut Shell,
    process: &ProcessBuilder,
    status: &str,
    translate: impl FnMut(&str) -> Option<(&'static str, String)>,
  ) -> Result<()> {
    run_tool(
      shell,
      &self.project.join(Self::LOG),
      process,
      status,
      translate,
    )
  }

  fn version_set(&self) -> Result<VersionSet> {
    let section = self.project.manifest.version.as_ref().ok_or_else(|| {
      hint(
        "packaging needs the project version",
        "add a `[version]` section to wkit.toml",
      )
    })?;
    VersionSet::load(&self.project.root, section)
  }

  fn is_cargo(project: &Project) -> bool {
    Builder::new(project, &BuildOptions::default()).system() == Some(BuildSystem::Cargo)
  }

  fn is_valid_set(set: &str) -> bool {
    set.starts_with(|c: char| c.is_ascii_lowercase())
      && set
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
  }
}

fn run_tool(
  shell: &mut Shell,
  log: &Path,
  process: &ProcessBuilder,
  status: &str,
  mut translate: impl FnMut(&str) -> Option<(&'static str, String)>,
) -> Result<()> {
  const TAIL: usize = 30;
  if let Some(parent) = log.parent() {
    fs_err::create_dir_all(parent)?;
  }
  let mut file = fs_err::File::create(log)?;
  shell.verbose(|s| s.status("Running", process))?;
  writeln!(file, "$ {process}")?;
  let verbose = shell.is_verbose();
  let bar = (!verbose).then(|| shell.transient(status));
  let mut tail = std::collections::VecDeque::with_capacity(TAIL);
  let mut failure = None;
  let exit = process.stream(|line| {
    let mut handle = || -> std::io::Result<()> {
      writeln!(file, "{line}")?;
      if verbose {
        return shell.passthrough(line);
      }
      if tail.len() == TAIL {
        tail.pop_front();
      }
      tail.push_back(line.to_owned());
      if let Some((status, message)) = translate(line) {
        return shell.status(status, message);
      }
      if let Some(bar) = &bar {
        bar.set_message(line.trim().to_owned());
      }
      Ok(())
    };
    if let Err(e) = handle() {
      failure.get_or_insert(e);
    }
  })?;
  if let Some(bar) = bar {
    bar.finish_and_clear();
  }
  if let Some(e) = failure {
    return Err(e.into());
  }
  if exit.success() {
    return Ok(());
  }
  for line in tail {
    shell.passthrough(line)?;
  }
  let tool = process.get_program().to_string_lossy();
  Err(
    Failure::new(
      format!(
        "`{tool}` failed ({exit})\n\nnote: the full log is in `{}`",
        log.display()
      ),
      exit.code().unwrap_or(Failure::INTERNAL),
    )
    .into(),
  )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Privileged {
  sudo: bool,
  pub dry_run: bool,
}

impl Privileged {
  pub fn new(dry_run: bool) -> Self {
    let root = ProcessBuilder::new("id")
      .arg("-u")
      .output()
      .is_ok_and(|output| output.stdout.trim_ascii() == b"0");
    Self {
      sudo: !root,
      dry_run,
    }
  }

  fn command(self, program: &str) -> ProcessBuilder {
    if self.sudo {
      ProcessBuilder::new("sudo").arg(program)
    } else {
      ProcessBuilder::new(program)
    }
  }

  fn run(self, shell: &mut Shell, process: &ProcessBuilder) -> Result<()> {
    if self.dry_run {
      shell.status("Would run", process)?;
      return Ok(());
    }
    shell.verbose(|s| s.status("Running", process))?;
    process.status()
  }
}

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

  pub fn detect() -> Self {
    Self::OS_RELEASE
      .iter()
      .find_map(|path| std::fs::read_to_string(path).ok())
      .map_or_else(
        || Self::Other(std::env::consts::OS.to_owned()),
        |text| Self::parse(&text),
      )
  }

  fn parse(os_release: &str) -> Self {
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
    if is(&["debian", "ubuntu", "linuxmint", "astra"]) {
      Self::Debian
    } else if is(&["arch", "manjaro", "endeavouros"]) {
      Self::Arch
    } else {
      let pretty = field("PRETTY_NAME");
      Self::Other(if pretty.is_empty() { id } else { pretty })
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSource {
  source: PathBuf,
  destination: PathBuf,
  mode: FileMode,
  config: bool,
}

impl FileSource {
  fn expand(root: &Path, mapping: &FileMapping, build_type: BuildType) -> Result<Vec<Self>> {
    let pattern = mapping.source(build_type);
    let sources = if mapping.is_glob() {
      Self::glob(root, &pattern)?
    } else {
      let source = root.join(&pattern);
      if !source.is_file() {
        return Err(hint(
          format!("`{pattern}` does not exist"),
          "check `from` in the `files` mapping",
        ));
      }
      vec![source]
    };
    Ok(
      sources
        .into_iter()
        .map(|source| Self {
          destination: mapping.destination(&source),
          mode: mapping.mode.unwrap_or_else(|| Self::default_mode(&source)),
          config: mapping.config,
          source,
        })
        .collect(),
    )
  }

  fn default_mode(path: &Path) -> FileMode {
    #[cfg(unix)]
    if std::fs::metadata(path)
      .is_ok_and(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0)
    {
      return FileMode::EXECUTABLE;
    }
    #[cfg(not(unix))]
    let _ = path;
    FileMode::REGULAR
  }

  fn glob(root: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let (dir, name) = pattern.rsplit_once('/').unwrap_or((".", pattern));
    let matcher = globset::Glob::new(name)
      .map_err(|e| anyhow::anyhow!("invalid pattern `{pattern}`: {e}"))?
      .compile_matcher();
    let mut found: Vec<_> = std::fs::read_dir(root.join(dir))
      .into_iter()
      .flatten()
      .flatten()
      .filter(|entry| {
        entry.file_type().is_ok_and(|t| t.is_file()) && matcher.is_match(entry.file_name())
      })
      .map(|entry| entry.path())
      .collect();
    anyhow::ensure!(!found.is_empty(), "`{pattern}` matches no files");
    found.sort();
    Ok(found)
  }
}

#[derive(Debug)]
struct Stage<'p> {
  packaging: &'p Packaging<'p>,
  format: &'static str,
  dir: PathBuf,
}

impl<'p> Stage<'p> {
  fn prepare(packaging: &'p Packaging<'p>, format: &'static str) -> Result<Self> {
    let dir = packaging.stage_dir(format);
    if dir.exists() {
      fs_err::remove_dir_all(&dir)?;
    }
    fs_err::create_dir_all(&dir)?;
    Ok(Self {
      packaging,
      format,
      dir,
    })
  }

  fn path(&self, absolute: &Path) -> PathBuf {
    self
      .dir
      .join(absolute.strip_prefix("/").unwrap_or(absolute))
  }

  fn install_tree(&self, shell: &mut Shell, build_dir: &Path, prefix: &Path) -> Result<()> {
    let project = self.packaging.project;
    let target = self.path(prefix);
    let install = ProcessBuilder::new("cmake")
      .cwd(&project.root)
      .arg("--install")
      .arg(build_dir)
      .arg("--prefix")
      .arg(&target)
      .args(["--config", &self.packaging.build_type().to_string()]);
    self
      .packaging
      .run_tool(shell, &install, "Installing", |_| None)?;
    for excluded in &project.manifest.package.exclude {
      let path = target.join(excluded);
      if path.is_dir() {
        fs_err::remove_dir_all(&path)?;
      } else if path.exists() {
        fs_err::remove_file(&path)?;
      }
    }
    Ok(())
  }

  fn add_files(&self) -> Result<()> {
    let project = self.packaging.project;
    for mapping in &project.manifest.package.files {
      for file in FileSource::expand(&project.root, mapping, self.packaging.build_type())? {
        Self::copy(&file.source, &self.path(&file.destination), file.mode.0)?;
      }
    }
    Ok(())
  }

  fn add_license(&self, destination: &Path) -> Result<bool> {
    let project = self.packaging.project;
    let license = match &project.manifest.package.license {
      Some(license) => Some(project.join(license)),
      None => Some(project.join("LICENSE")).filter(|path| path.is_file()),
    };
    let Some(license) = license else {
      return Ok(false);
    };
    Self::copy(&license, &self.path(destination), 0o644)?;
    Ok(true)
  }

  fn run_hook(&self, shell: &mut Shell, build_dir: Option<&Path>) -> Result<()> {
    let project = self.packaging.project;
    let Some(hook) = &project.manifest.package.stage else {
      return Ok(());
    };
    let mut process = self
      .packaging
      .with_env(hook.to_process(&project.root, &[]))
      .env("WKIT_STAGE_DIR", &self.dir)
      .env("WKIT_PACKAGE_FORMAT", self.format);
    if let Some(build_dir) = build_dir {
      process = process.env("WKIT_BUILD_DIR", build_dir);
    }
    shell.status("Staging", format_args!("running `{}`", hook.run.line(&[])))?;
    process.status()
  }

  fn copy(source: &Path, target: &Path, mode: u32) -> Result<()> {
    if let Some(parent) = target.parent() {
      fs_err::create_dir_all(parent)?;
    }
    fs_err::copy(source, target)?;
    #[cfg(unix)]
    fs_err::set_permissions(target, std::os::unix::fs::PermissionsExt::from_mode(mode))?;
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
  }
}

pub fn windows(
  packaging: &Packaging<'_>,
  shell: &mut Shell,
  extra: &[String],
) -> Result<Vec<PathBuf>> {
  const CPACK_CONFIG: &str = "CPackConfig.cmake";
  anyhow::ensure!(
    which::which("cpack").is_ok(),
    "Windows packages are built with CPack, which is not installed"
  );
  let project = packaging.project;
  let config = &project.manifest.package.windows;
  let build_dir = packaging.build_native(shell)?;
  if !build_dir.join(CPACK_CONFIG).is_file() {
    return Err(hint(
      format!("`{}` has no {CPACK_CONFIG}", packaging.shown(&build_dir)),
      "add `include(CPack)` to CMakeLists.txt",
    ));
  }
  let version = packaging.version()?;
  shell.status(
    "Packaging",
    format_args!("{} v{version} ({})", packaging.name, config.generator),
  )?;
  let stage = Stage::prepare(packaging, "windows")?;
  stage.install_tree(shell, &build_dir, Path::new("/"))?;
  stage.add_files()?;
  let license = Path::new("/LICENSE");
  let license = stage.add_license(license)?.then(|| stage.path(license));
  stage.run_hook(shell, Some(&build_dir))?;
  fs_err::create_dir_all(&packaging.output)?;

  let mut defines = vec![
    ("CPACK_INSTALL_CMAKE_PROJECTS", String::new()),
    (
      "CPACK_INSTALLED_DIRECTORIES",
      format!("{};/", stage.dir.display()),
    ),
    ("CPACK_PACKAGE_NAME", packaging.name.clone()),
    ("CPACK_PACKAGE_VERSION", version.render(Flavor::Numeric)),
    (
      "CPACK_PACKAGE_FILE_NAME",
      format!("{}-{version}-win64", packaging.name),
    ),
  ];
  if let Some(license) = license {
    defines.push(("CPACK_RESOURCE_FILE_LICENSE", license.display().to_string()));
  }
  if let Some(icon) = &config.icon {
    let icon = project.join(icon).display().to_string();
    match config.generator {
      WindowsGenerator::Nsis => {
        defines.push(("CPACK_NSIS_MUI_ICON", icon.clone()));
        defines.push(("CPACK_NSIS_MUI_UNIICON", icon));
      }
      WindowsGenerator::Wix => defines.push(("CPACK_WIX_PRODUCT_ICON", icon)),
      WindowsGenerator::Zip => {}
    }
  }
  let cpack = ProcessBuilder::new("cpack")
    .cwd(&project.root)
    .arg("--config")
    .arg(build_dir.join(CPACK_CONFIG))
    .args(["-G", &config.generator.to_string(), "-B"])
    .arg(&packaging.output)
    .args(["-C", &packaging.build_type().to_string()])
    .args(
      defines
        .into_iter()
        .flat_map(|(key, value)| ["-D".to_owned(), format!("{key}={value}")]),
    )
    .args(extra);
  let mut produced = Vec::new();
  packaging.run_tool(shell, &cpack, "Packaging", |line| {
    if let Some(path) = line
      .strip_prefix("CPack: - package: ")
      .and_then(|rest| rest.strip_suffix(" generated."))
    {
      produced.push(PathBuf::from(path));
    }
    None
  })?;
  let scratch = packaging.output.join("_CPack_Packages");
  if scratch.is_dir() {
    fs_err::remove_dir_all(scratch)?;
  }
  for package in &produced {
    shell.status("Packaged", packaging.shown(package))?;
  }
  Ok(produced)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    std::fs::write(
      dir.path().join("CMakeLists.txt"),
      "project(x VERSION 1.2.3-rc.1)\n",
    )
    .unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn defaults_to_release_and_the_project_name() {
    let (dir, project) = project(
      "[project]\nname = \"crc8\"\nlanguages = [\"c\"]\n\n[version]\nfiles = [{ path = \"CMakeLists.txt\", pattern = 'VERSION (?<version>\\S+)\\)' }]\n",
    );
    let packaging = Packaging::new(&project, BuildOptions::default(), None, None).unwrap();
    assert_eq!(packaging.name, "crc8");
    assert_eq!(packaging.build_type(), BuildType::Release);
    assert_eq!(packaging.output, dir.path().join("build/package"));
    assert_eq!(packaging.version().unwrap().to_string(), "1.2.3-rc.1");
    assert_eq!(
      packaging.stage_dir("deb"),
      dir.path().join("build/stage/deb/crc8")
    );
  }

  #[test]
  fn sets_need_a_conan_option() {
    let (_dir, project) = project("[project]\nlanguages = [\"c\"]\n");
    let err = Packaging::new(
      &project,
      BuildOptions::default(),
      Some("field".into()),
      None,
    )
    .unwrap_err();
    assert!(
      err.to_string().contains("set `package.set-option`"),
      "{err}"
    );
    let packaging = Packaging::new(&project, BuildOptions::default(), None, None).unwrap();
    assert!(
      packaging
        .version()
        .unwrap_err()
        .to_string()
        .contains("add a `[version]` section")
    );
    assert!(Packaging::is_valid_set("field_ops2"));
    assert!(!Packaging::is_valid_set("Field") && !Packaging::is_valid_set("2x"));
  }

  #[cfg(unix)]
  #[test]
  fn tool_failures_show_the_tail_and_keep_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("logs/tool.log");
    let mut shell = Shell::sink();
    let mut seen = Vec::new();
    run_tool(
      &mut shell,
      &log,
      &ProcessBuilder::shell("echo '==> one'; echo two"),
      "Running",
      |line| {
        seen.push(line.to_owned());
        line
          .strip_prefix("==> ")
          .map(|rest| ("Step", rest.to_owned()))
      },
    )
    .unwrap();
    assert_eq!(seen, ["==> one", "two"]);
    assert!(std::fs::read_to_string(&log).unwrap().contains("two\n"));
    let err = run_tool(
      &mut shell,
      &log,
      &ProcessBuilder::shell("echo boom; exit 3"),
      "Running",
      |_| None,
    )
    .unwrap_err();
    assert_eq!(Failure::exit_code(&err), 3);
    assert!(std::fs::read_to_string(&log).unwrap().contains("boom\n"));
  }

  #[test]
  fn sudo_wraps_commands_for_regular_users() {
    let privileged = Privileged {
      sudo: true,
      dry_run: true,
    };
    assert_eq!(
      privileged.command("install").arg("-d").to_string(),
      "sudo install -d"
    );
    let root = Privileged {
      sudo: false,
      ..privileged
    };
    assert_eq!(root.command("install").to_string(), "install");
  }

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

  #[test]
  fn globs_expand_into_directories() {
    let mapping = |from: &str, to: &str| FileMapping {
      from: from.into(),
      to: to.into(),
      mode: None,
      config: false,
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("lua")).unwrap();
    for name in ["b.lua", "a.lua", "notes.txt"] {
      std::fs::write(dir.path().join("lua").join(name), "").unwrap();
    }
    let found = FileSource::expand(
      dir.path(),
      &mapping("lua/*.lua", "/etc/relay/lua/"),
      BuildType::Release,
    )
    .unwrap();
    let destinations: Vec<_> = found.iter().map(|f| f.destination.clone()).collect();
    assert_eq!(
      destinations,
      [
        PathBuf::from("/etc/relay/lua/a.lua"),
        "/etc/relay/lua/b.lua".into()
      ]
    );
    assert_eq!(found[0].mode, FileMode::REGULAR);
    let err = FileSource::expand(
      dir.path(),
      &mapping("lua/*.so", "/usr/lib/"),
      BuildType::Release,
    )
    .unwrap_err();
    assert!(err.to_string().contains("matches no files"), "{err}");
    let err = FileSource::expand(
      dir.path(),
      &mapping("missing", "/usr/bin/x"),
      BuildType::Release,
    )
    .unwrap_err();
    assert!(
      err.to_string().contains("`missing` does not exist"),
      "{err}"
    );
  }
}
