use wkit_core::{Language, detect::BuildSystem, fs::SourceSet};
use wkit_manifest::{TestRunner, VersionFile};
use wkit_version::VersionDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
  pub name: String,
  pub languages: std::collections::BTreeSet<Language>,
  pub build_system: Option<BuildSystem>,
  pub version_files: Vec<String>,
  pub test_runners: Vec<TestRunner>,
}

impl Detection {
  const ROOT_VERSION_FILES: &[&str] = &["conanfile.py", "Cargo.toml", "CMakeLists.txt"];
  const NESTED_VERSION_FILES: &[&str] = &["PKGBUILD", "meta.lua"];
  const CTEST_MARKERS: &[&str] = &[
    "enable_testing(",
    "include(ctest)",
    "add_test(",
    "catch_discover_tests(",
    "gtest_discover_tests(",
  ];

  pub fn scan(root: &std::path::Path) -> anyhow::Result<Self> {
    let languages = Language::detect(root)?;
    anyhow::ensure!(
      !languages.is_empty(),
      "could not find C, C++ or Rust sources in `{}`",
      root.display()
    );
    Ok(Self {
      name: root.file_name().map_or_else(
        || "project".to_owned(),
        |n| n.to_string_lossy().into_owned(),
      ),
      build_system: BuildSystem::from_path(root),
      version_files: Self::version_files(root)?,
      test_runners: Self::test_runners(root, &languages)?,
      languages,
    })
  }

  pub fn render(&self) -> String {
    let quote = |s: &str| toml::Value::String(s.to_owned()).to_string();
    let list = |items: Vec<String>| items.join(", ");
    let languages = self
      .languages
      .iter()
      .map(|l| match l {
        Language::C => quote("c"),
        Language::Cpp => quote("cpp"),
        Language::Rust => quote("rust"),
      })
      .collect();
    let mut sections = vec![format!(
      "[project]\nname = {}\nlanguages = [{}]\n",
      quote(&self.name),
      list(languages)
    )];
    if !self.version_files.is_empty() {
      let files = self.version_files.iter().map(|f| quote(f)).collect();
      sections.push(format!("[version]\nfiles = [{}]\n", list(files)));
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

  fn version_files(root: &std::path::Path) -> anyhow::Result<Vec<String>> {
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

  fn test_runners(
    root: &std::path::Path,
    languages: &std::collections::BTreeSet<Language>,
  ) -> anyhow::Result<Vec<TestRunner>> {
    let mut runners = Vec::new();
    if languages.contains(&Language::Rust) {
      runners.push(if root.join(".config/nextest.toml").is_file() {
        TestRunner::Nextest
      } else {
        TestRunner::Cargo
      });
    }
    if languages.iter().any(|l| l.is_c_family())
      && let Some(cmake) = wkit_core::fs::read_optional(&root.join("CMakeLists.txt"))?
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
  use wkit_manifest::Manifest;

  use super::*;

  fn write(root: &std::path::Path, path: &str, text: &str) {
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
    assert!(manifest.manifest.has(Language::Cpp) && manifest.manifest.has(Language::Rust));
    assert_eq!(manifest.manifest.version.unwrap().files.len(), 4);
  }

  #[test]
  fn requires_sources() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Detection::scan(dir.path()).is_err());
  }
}
