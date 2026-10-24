use wkit_core::{ProcessBuilder, Shell, version::Flavor};
use wkit_manifest::package::WindowsGenerator;

use crate::{Error, Packaging, Result, Stage, ToolRun};

#[derive(Debug, Clone, Copy)]
pub struct WindowsPackager<'p> {
  packaging: &'p Packaging<'p>,
}

impl<'p> WindowsPackager<'p> {
  pub const CPACK_CONFIG: &'static str = "CPackConfig.cmake";

  pub fn new(packaging: &'p Packaging<'p>) -> Self {
    Self { packaging }
  }

  pub fn command(
    &self,
    build_dir: &std::path::Path,
    stage: &std::path::Path,
    license: Option<&std::path::Path>,
    extra: &[String],
  ) -> Result<ProcessBuilder> {
    let packaging = self.packaging;
    let project = packaging.project();
    let config = &project.manifest.package.windows;
    let version = packaging.version()?;
    let mut defines = vec![
      ("CPACK_INSTALL_CMAKE_PROJECTS", String::new()),
      (
        "CPACK_INSTALLED_DIRECTORIES",
        format!("{};/", stage.display()),
      ),
      ("CPACK_PACKAGE_NAME", packaging.name().to_owned()),
      ("CPACK_PACKAGE_VERSION", version.render(Flavor::Numeric)),
      (
        "CPACK_PACKAGE_FILE_NAME",
        format!("{}-{version}-win64", packaging.name()),
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
      .arg(build_dir.join(Self::CPACK_CONFIG))
      .args(["-G", &config.generator.to_string(), "-B"])
      .arg(packaging.output())
      .args(["-C", &packaging.build_type().to_string()])
      .args(
        defines
          .into_iter()
          .flat_map(|(key, value)| ["-D".to_owned(), format!("{key}={value}")]),
      );
    Ok(cpack.args(extra))
  }

  pub fn run(&self, shell: &mut Shell, extra: &[String]) -> Result<Vec<std::path::PathBuf>> {
    let packaging = self.packaging;
    let project = packaging.project();
    if which::which("cpack").is_err() {
      return Err(Error::setup(
        "Windows packages are built with CPack, which is not installed",
        None,
      ));
    }
    let build_dir = packaging.build_native(shell)?;
    if !build_dir.join(Self::CPACK_CONFIG).is_file() {
      return Err(Error::setup(
        format!(
          "`{}` has no {}",
          packaging.shown(&build_dir),
          Self::CPACK_CONFIG
        ),
        Some("add `include(CPack)` to CMakeLists.txt".to_owned()),
      ));
    }
    let generator = project.manifest.package.windows.generator;
    shell.status(
      "Packaging",
      format_args!(
        "{} v{} ({generator})",
        packaging.name(),
        packaging.version()?
      ),
    )?;
    let stage = Stage::prepare(packaging, "windows")?;
    stage.install_tree(shell, &build_dir, std::path::Path::new("/"))?;
    stage.add_files()?;
    let license = std::path::Path::new("/LICENSE");
    let license = stage.add_license(license)?.then(|| stage.path(license));
    stage.run_hook(shell, Some(&build_dir))?;
    std::fs::create_dir_all(packaging.output())?;
    let mut produced = Vec::new();
    let cpack = self.command(&build_dir, stage.dir(), license.as_deref(), extra)?;
    ToolRun::new(shell, packaging.log_path())?.run(&cpack, "Packaging", |line| {
      if let Some(path) = line
        .strip_prefix("CPack: - package: ")
        .and_then(|rest| rest.strip_suffix(" generated."))
      {
        produced.push(std::path::PathBuf::from(path));
      }
      None
    })?;
    let scratch = packaging.output().join("_CPack_Packages");
    if scratch.is_dir() {
      std::fs::remove_dir_all(scratch)?;
    }
    for package in &produced {
      shell.status("Packaged", packaging.shown(package))?;
    }
    Ok(produced)
  }
}
