#[derive(Debug, Clone, Copy)]
pub struct CompileCommands;

impl CompileCommands {
  pub const FILE: &str = "compile_commands.json";

  pub fn link(root: &std::path::Path, build_dir: &std::path::Path) -> std::io::Result<bool> {
    let target = build_dir.join(Self::FILE);
    let link = root.join(Self::FILE);
    if !target.is_file() || target == link {
      return Ok(false);
    }
    match std::fs::symlink_metadata(&link) {
      Ok(meta) if !meta.file_type().is_symlink() => return Ok(false),
      Ok(_) => {
        if std::fs::read_link(&link).is_ok_and(|current| Self::resolves(root, &current, &target)) {
          return Ok(false);
        }
        std::fs::remove_file(&link)?;
      }
      Err(_) => {}
    }
    let relative = target.strip_prefix(root).unwrap_or(&target);
    Self::create(relative, &link)?;
    Ok(true)
  }

  fn resolves(root: &std::path::Path, current: &std::path::Path, target: &std::path::Path) -> bool {
    root.join(current) == target
  }

  #[cfg(unix)]
  fn create(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
  }

  #[cfg(windows)]
  fn create(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;

  #[test]
  fn links_the_active_build_and_leaves_real_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for build in ["build/Debug", "build/Release"] {
      std::fs::create_dir_all(root.join(build)).unwrap();
      std::fs::write(root.join(build).join(CompileCommands::FILE), build).unwrap();
    }
    let read = || std::fs::read_to_string(root.join(CompileCommands::FILE)).unwrap();
    assert!(CompileCommands::link(root, &root.join("build/Debug")).unwrap());
    assert_eq!(read(), "build/Debug");
    assert!(!CompileCommands::link(root, &root.join("build/Debug")).unwrap());
    assert!(CompileCommands::link(root, &root.join("build/Release")).unwrap());
    assert_eq!(read(), "build/Release");
    assert_eq!(
      std::fs::read_link(root.join(CompileCommands::FILE)).unwrap(),
      std::path::Path::new("build/Release/compile_commands.json")
    );

    std::fs::remove_file(root.join(CompileCommands::FILE)).unwrap();
    std::fs::write(root.join(CompileCommands::FILE), "mine").unwrap();
    assert!(!CompileCommands::link(root, &root.join("build/Debug")).unwrap());
    assert_eq!(read(), "mine");
  }
}
