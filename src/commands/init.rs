use std::{collections::BTreeSet, path::Path};

use anyhow::Context as _;

use super::Run;
use crate::{
  Language, Result,
  cli::Context,
  detect::BuildSystem,
  fs::SourceSet,
  manifest::{MANIFEST_FILE, Manifest, TestRunner, VersionFile},
  shell::plural,
  version::{CalVer, Scheme, VersionDocument},
};

#[derive(Debug, clap::Args)]
pub struct InitArgs {
  /// Overwrite an existing wkit.toml
  #[arg(long)]
  force: bool,
}

impl Run for InitArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let path = ctx.cwd.join(
      ctx
        .manifest_path
        .as_deref()
        .unwrap_or(Path::new(MANIFEST_FILE)),
    );
    anyhow::ensure!(
      self.force || !path.exists(),
      "`{}` already exists; pass `--force` to overwrite it",
      path.display()
    );
    let detection = Detection::scan(path.parent().unwrap_or(&ctx.cwd))?;
    let text = detection.render();
    Manifest::parse(&text).context("generated an invalid manifest; this is a bug")?;
    fs_err::write(&path, &text)?;
    let languages: Vec<_> = detection
      .languages
      .iter()
      .map(ToString::to_string)
      .collect();
    ctx.shell.status(
      "Created",
      format_args!(
        "`{}` ({}; {})",
        path.strip_prefix(&ctx.cwd).unwrap_or(&path).display(),
        languages.join(", "),
        plural(detection.version_files.len(), "version file")
      ),
    )?;
    Ok(())
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Detection {
  name: String,
  languages: BTreeSet<Language>,
  build_system: Option<BuildSystem>,
  version_files: Vec<String>,
  scheme: Scheme,
  test_runners: Vec<TestRunner>,
}

impl Detection {
  const ROOT_VERSION_FILES: &[&str] = &["conanfile.py", "Cargo.toml", "CMakeLists.txt"];
  const NESTED_VERSION_FILES: &[&str] = &["PKGBUILD", "meta.lua", "conanfile.py", "CMakeLists.txt"];
  const CTEST_MARKERS: &[&str] = &[
    "enable_testing(",
    "include(ctest)",
    "add_test(",
    "catch_discover_tests(",
    "gtest_discover_tests(",
  ];

  fn scan(root: &Path) -> Result<Self> {
    let languages = Language::detect(root)?;
    anyhow::ensure!(
      !languages.is_empty(),
      "could not find C, C++ or Rust sources in `{}`",
      root.display()
    );
    let version_files = Self::version_files(root)?;
    let scheme = Self::scheme(root, version_files.first())?;
    Ok(Self {
      name: root.file_name().map_or_else(
        || "project".to_owned(),
        |n| n.to_string_lossy().into_owned(),
      ),
      build_system: BuildSystem::from_path(root),
      version_files,
      scheme,
      test_runners: Self::test_runners(root, &languages)?,
      languages,
    })
  }

  fn render(&self) -> String {
    let quote = |s: &str| toml::Value::from(s).to_string();
    let languages: Vec<_> = self
      .languages
      .iter()
      .map(|l| {
        quote(match l {
          Language::C => "c",
          Language::Cpp => "cpp",
          Language::Rust => "rust",
        })
      })
      .collect();
    let mut sections = vec![format!(
      "[project]\nname = {}\nlanguages = [{}]\n",
      quote(&self.name),
      languages.join(", ")
    )];
    if !self.version_files.is_empty() {
      let files: Vec<_> = self.version_files.iter().map(|f| quote(f)).collect();
      let scheme = match self.scheme {
        Scheme::SemVer => String::new(),
        Scheme::CalVer => format!("scheme = {}\n", quote(&self.scheme.to_string())),
      };
      sections.push(format!(
        "[version]\n{scheme}files = [{}]\n",
        files.join(", ")
      ));
    }
    if let Some(system) = self.build_system {
      sections.push(format!(
        "[build]\nsystem = {}\n",
        quote(&system.to_string())
      ));
    }
    for runner in &self.test_runners {
      sections.push(format!(
        "[[test.suite]]\nrunner = {}\n",
        quote(&runner.to_string())
      ));
    }
    sections.join("\n")
  }

  fn version_files(root: &Path) -> Result<Vec<String>> {
    let nested = SourceSet::new(root)
      .include(Self::NESTED_VERSION_FILES.iter().copied())
      .exclude(
        SourceSet::DEFAULT_EXCLUDES
          .iter()
          .copied()
          .filter(|&e| e != "/packaging/"),
      )
      .collect()?;
    let mut nested: Vec<String> = nested
      .iter()
      .filter(|p| p.parent() != Some(root))
      .filter(|p| !p.ends_with("CMakeLists.txt") || p.with_file_name("conanfile.py").is_file())
      .filter_map(|p| p.strip_prefix(root).ok())
      .map(|p| {
        p.components()
          .map(|c| c.as_os_str().to_string_lossy())
          .collect::<Vec<_>>()
          .join("/")
      })
      .collect();
    nested.sort();
    Ok(
      Self::ROOT_VERSION_FILES
        .iter()
        .map(|&f| f.to_owned())
        .chain(nested)
        .filter(|path| {
          VersionFile::infer(path.as_str())
            .is_some_and(|file| VersionDocument::load(root, &file).is_ok())
        })
        .collect(),
    )
  }

  fn scheme(root: &Path, source: Option<&String>) -> Result<Scheme> {
    let Some(file) = source.and_then(|s| VersionFile::infer(s.as_str())) else {
      return Ok(Scheme::SemVer);
    };
    let calver = VersionDocument::load(root, &file)?
      .current()
      .parse::<CalVer>()
      .is_ok();
    Ok(if calver {
      Scheme::CalVer
    } else {
      Scheme::SemVer
    })
  }

  fn test_runners(root: &Path, languages: &BTreeSet<Language>) -> Result<Vec<TestRunner>> {
    let mut runners = Vec::new();
    if languages.contains(&Language::Rust) {
      let nextest = root.join(".config/nextest.toml").is_file();
      runners.push(if nextest {
        TestRunner::Nextest
      } else {
        TestRunner::Cargo
      });
    }
    if languages.iter().any(|l| l.is_c_family())
      && let Some(cmake) = crate::fs::read_optional(&root.join("CMakeLists.txt"))?
    {
      let cmake = cmake.to_ascii_lowercase().replace([' ', '\t'], "");
      if Self::CTEST_MARKERS.iter().any(|m| cmake.contains(m)) {
        runners.push(TestRunner::Ctest);
      }
    }
    Ok(runners)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }

  #[test]
  fn detects_a_mixed_conan_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
      root,
      "conanfile.py",
      "class P(ConanFile):\n    version = \"1.0.0\"\n",
    );
    write(
      root,
      "CMakeLists.txt",
      "project(p VERSION 1.0.0)\nenable_testing()\n",
    );
    write(root, "Cargo.toml", "[workspace]\nmembers = []\n");
    write(root, "src/main.cpp", "int main() {}\n");
    write(root, "packaging/arch/PKGBUILD", "pkgver=1.0.0\npkgrel=1\n");
    write(
      root,
      "src/plugin/meta.lua",
      "return {\n  version = \"1.0.0\",\n}\n",
    );
    write(root, "build/meta.lua", "version = '9.9.9'\n");
    write(root, ".config/nextest.toml", "");

    let detection = Detection::scan(root).unwrap();
    assert_eq!(
      detection.version_files,
      [
        "conanfile.py",
        "CMakeLists.txt",
        "packaging/arch/PKGBUILD",
        "src/plugin/meta.lua"
      ]
    );
    assert_eq!(detection.build_system, Some(BuildSystem::Conan));
    assert_eq!(
      detection.test_runners,
      [TestRunner::Nextest, TestRunner::Ctest]
    );

    let manifest = Manifest::parse(&detection.render()).unwrap();
    assert!(manifest.warnings.is_empty());
    assert!(manifest.has(Language::Cpp) && manifest.has(Language::Rust));
    assert_eq!(manifest.version.unwrap().files.len(), 4);
  }

  #[test]
  fn detects_conan_workspace_packages() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "conanws.yml", "packages: []\n");
    for package in ["core", "app"] {
      write(
        root,
        &format!("{package}/conanfile.py"),
        "class P(ConanFile):\n  version = \"0.5.0\"\n",
      );
      write(
        root,
        &format!("{package}/CMakeLists.txt"),
        "project(p VERSION 0.5.0)\n",
      );
      write(root, &format!("{package}/src/main.cpp"), "int main() {}\n");
    }
    write(
      root,
      "third_party/zstd/CMakeLists.txt",
      "project(zstd VERSION 1.5.7)\n",
    );
    write(
      root,
      "core/tests/CMakeLists.txt",
      "add_executable(t t.cpp)\n",
    );
    assert_eq!(
      Detection::scan(root).unwrap().version_files,
      [
        "app/CMakeLists.txt",
        "app/conanfile.py",
        "core/CMakeLists.txt",
        "core/conanfile.py"
      ]
    );
  }

  #[test]
  fn detects_calendar_versions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "CMakeLists.txt", "project(p VERSION 2026.10.24.2)\n");
    write(root, "main.c", "int main(void) { return 0; }\n");
    let detection = Detection::scan(root).unwrap();
    assert_eq!(detection.scheme, Scheme::CalVer);
    let manifest = Manifest::parse(&detection.render()).unwrap();
    assert_eq!(manifest.version.unwrap().scheme, Scheme::CalVer);
  }

  #[test]
  fn requires_sources() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Detection::scan(dir.path()).is_err());
  }
}
