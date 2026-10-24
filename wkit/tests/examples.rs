const SKIPPED: &[&str] = &["build", "target", ".git", "compile_commands.json"];

fn examples() -> Vec<std::path::PathBuf> {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests");
  let mut examples: Vec<_> = std::fs::read_dir(&root)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.join("wkit.toml").is_file())
    .collect();
  examples.sort();
  assert!(
    examples.len() >= 8,
    "examples are missing from {}",
    root.display()
  );
  examples
}

fn wkit(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
  std::process::Command::new(env!("CARGO_BIN_EXE_wkit"))
    .arg("-C")
    .arg(dir)
    .args(args)
    .env("WKIT_COLOR", "never")
    .output()
    .unwrap()
}

fn succeed(dir: &std::path::Path, args: &[&str]) -> (String, String) {
  let output = wkit(dir, args);
  let stdout = String::from_utf8(output.stdout).unwrap();
  let stderr = String::from_utf8(output.stderr).unwrap();
  assert!(
    output.status.success(),
    "`wkit {}` failed in {}:\n{stderr}",
    args.join(" "),
    dir.display()
  );
  (stdout, stderr)
}

fn copy(from: &std::path::Path, to: &std::path::Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap() {
    let entry = entry.unwrap();
    let name = entry.file_name();
    if SKIPPED.iter().any(|s| name == *s) {
      continue;
    }
    if entry.file_type().unwrap().is_dir() {
      copy(&entry.path(), &to.join(&name));
    } else {
      std::fs::copy(entry.path(), to.join(&name)).unwrap();
    }
  }
}

fn languages(manifest: &str) -> String {
  manifest
    .lines()
    .find(|l| l.starts_with("languages"))
    .unwrap()
    .to_owned()
}

#[test]
fn every_example_has_consistent_versions() {
  for example in examples() {
    let (stdout, stderr) = succeed(&example, &["bump"]);
    assert_eq!(stderr, "", "{}", example.display());
    assert!(!stdout.is_empty(), "{}", example.display());
  }
}

#[test]
fn bumping_an_example_keeps_it_consistent() {
  for example in examples() {
    let dir = tempfile::tempdir().unwrap();
    copy(&example, dir.path());
    let calver = std::fs::read_to_string(example.join("wkit.toml"))
      .unwrap()
      .contains("scheme = \"calver\"");
    let bump: &[&str] = if calver {
      &["bump", "-p"]
    } else {
      &["bump", "--pre", "rc"]
    };
    let (version, _) = succeed(dir.path(), bump);
    let (stdout, stderr) = succeed(dir.path(), &["bump"]);
    assert_eq!(stderr, "", "{}", example.display());
    let core = version.trim().split('-').next().unwrap();
    assert!(stdout.contains(core), "{}: {stdout}", example.display());
  }
}

#[test]
fn init_rediscovers_each_example() {
  for example in examples() {
    let dir = tempfile::tempdir().unwrap();
    copy(&example, dir.path());
    let original = std::fs::read_to_string(dir.path().join("wkit.toml")).unwrap();
    std::fs::remove_file(dir.path().join("wkit.toml")).unwrap();
    succeed(dir.path(), &["init"]);
    let generated = std::fs::read_to_string(dir.path().join("wkit.toml")).unwrap();
    assert_eq!(
      languages(&generated),
      languages(&original),
      "{}",
      example.display()
    );
    succeed(dir.path(), &["bump"]);
  }
}

#[test]
fn clean_dry_run_works_everywhere() {
  for example in examples() {
    let (_, stderr) = succeed(&example, &["clean", "--dry-run"]);
    assert!(stderr.contains("no files deleted"), "{}", example.display());
  }
}

#[test]
fn static_checks_pass_on_every_example() {
  for example in examples() {
    let (_, stderr) = succeed(&example, &["check", "--only", "versions"]);
    assert!(
      stderr.contains("ok versions"),
      "{}: {stderr}",
      example.display()
    );
    succeed(&example, &["check", "--only", "secrets,pre-commit"]);
  }
}

#[test]
fn check_fails_on_a_leaked_token() {
  let dir = tempfile::tempdir().unwrap();
  copy(&examples()[0], dir.path());
  let token = format!("ghp_{}", "a1B2".repeat(9));
  std::fs::write(dir.path().join("notes.txt"), format!("token = {token}\n")).unwrap();
  let output = wkit(dir.path(), &["check", "--only", "secrets"]);
  let stderr = String::from_utf8(output.stderr).unwrap();
  assert_eq!(output.status.code(), Some(1), "{stderr}");
  assert!(stderr.contains("notes.txt:1"), "{stderr}");
  assert!(stderr.contains("error: checks failed: secrets"), "{stderr}");
}

#[test]
fn test_rejects_unknown_suites_with_a_hint() {
  let example = examples()
    .into_iter()
    .find(|e| e.ends_with("conan-lib"))
    .unwrap();
  let output = wkit(&example, &["test", "--suite", "nope"]);
  let stderr = String::from_utf8(output.stderr).unwrap();
  assert!(!output.status.success());
  assert!(stderr.contains("unknown test suite `nope`"), "{stderr}");
  assert!(
    stderr.contains("help: expected one of: unit, ctest"),
    "{stderr}"
  );
}

#[test]
fn plain_cmake_example_builds_before_testing() {
  if which::which("cmake").is_err() {
    return;
  }
  let example = examples()
    .into_iter()
    .find(|e| e.ends_with("cmake-c"))
    .unwrap();
  let dir = tempfile::tempdir().unwrap();
  copy(&example, dir.path());
  let (_, stderr) = succeed(dir.path(), &["test"]);
  assert!(stderr.contains("Compiling crc8 v0.3.0 (Debug)"), "{stderr}");
  assert!(stderr.contains("Finished `Debug` build"), "{stderr}");
  assert!(stderr.contains("ok ctest"), "{stderr}");

  let (_, stderr) = succeed(dir.path(), &["build", "--build-type", "release"]);
  assert!(stderr.contains("`Release` build"), "{stderr}");
  assert!(!stderr.contains("Building"), "{stderr}");
}

#[test]
fn plain_cmake_example_runs_and_links_compile_commands() {
  if which::which("cmake").is_err() {
    return;
  }
  let example = examples()
    .into_iter()
    .find(|e| e.ends_with("cmake-c"))
    .unwrap();
  let dir = tempfile::tempdir().unwrap();
  copy(&example, dir.path());
  succeed(dir.path(), &["configure"]);
  if cfg!(unix) {
    assert_eq!(
      std::fs::read_link(dir.path().join("compile_commands.json")).unwrap(),
      std::path::Path::new("build/default/compile_commands.json")
    );
  }
  let (stdout, stderr) = succeed(dir.path(), &["run", "--", "world"]);
  assert_eq!(stdout, "92  hello\nb3  world\n");
  assert!(
    stderr.contains("Running `build/default/crc8sum"),
    "{stderr}"
  );
}

#[test]
fn clean_removes_workspace_package_outputs() {
  let example = examples()
    .into_iter()
    .find(|e| e.ends_with("conan-workspace"))
    .unwrap();
  let dir = tempfile::tempdir().unwrap();
  copy(&example, dir.path());
  std::fs::create_dir_all(dir.path().join("core/test_package")).unwrap();
  std::fs::write(dir.path().join("core/test_package/conanfile.py"), "").unwrap();
  let outputs = [
    "build/Debug",
    "app/build/Debug",
    "core/build/Debug",
    "core/test_package/build/Debug",
  ];
  for output in outputs {
    std::fs::create_dir_all(dir.path().join(output)).unwrap();
  }
  for package in ["app", "core"] {
    std::fs::write(dir.path().join(package).join("CMakeUserPresets.json"), "{}").unwrap();
  }
  succeed(dir.path(), &["clean"]);
  for leftover in [
    "build",
    "app/build",
    "core/build",
    "app/CMakeUserPresets.json",
  ] {
    assert!(!dir.path().join(leftover).exists(), "{leftover}");
  }
  assert!(dir.path().join("core/conanfile.py").is_file());
}

#[test]
fn cmake_example_packages_and_plans_an_install() {
  if which::which("cmake").is_err() {
    return;
  }
  let example = examples()
    .into_iter()
    .find(|e| e.ends_with("cmake-c"))
    .unwrap();
  let dir = tempfile::tempdir().unwrap();
  copy(&example, dir.path());
  let manifest = dir.path().join("wkit.toml");
  let toml = std::fs::read_to_string(&manifest).unwrap().replace(
    "[package]\n",
    "[package]\nstage = 'echo \"$WKIT_PACKAGE_FORMAT $WKIT_PACKAGE_VERSION\" > \"$WKIT_STAGE_DIR/usr/share/doc/crc8/stamp\"'\n",
  );
  std::fs::write(&manifest, toml).unwrap();

  let (_, stderr) = succeed(dir.path(), &["package", "deb"]);
  assert!(stderr.contains("Packaging crc8 v0.3.0 (deb)"), "{stderr}");
  let deb = dir.path().join("build/package/crc8_0.3.0_amd64.deb");
  assert!(std::fs::read(&deb).unwrap().starts_with(b"!<arch>\n"));
  let stamp = dir
    .path()
    .join("build/stage/deb/crc8/usr/share/doc/crc8/stamp");
  assert_eq!(std::fs::read_to_string(stamp).unwrap(), "deb 0.3.0\n");

  let (_, stderr) = succeed(dir.path(), &["install", "--files", "--dry-run"]);
  assert!(stderr.contains("Would run"), "{stderr}");
  assert!(
    stderr.contains("install -Dm0755") && stderr.contains("/usr/local/bin/crc8sum"),
    "{stderr}"
  );
  let (_, stderr) = succeed(dir.path(), &["uninstall", "--dry-run"]);
  assert!(stderr.contains("nothing to remove"), "{stderr}");

  if which::which("cpack").is_ok() {
    let (_, stderr) = succeed(dir.path(), &["package", "windows"]);
    assert!(stderr.contains("crc8-0.3.0-win64.zip"), "{stderr}");
    assert!(!dir.path().join("build/package/_CPack_Packages").exists());
  }
}

#[test]
fn packaging_without_a_maintainer_explains_the_fix() {
  let example = examples()
    .into_iter()
    .find(|e| e.ends_with("conan-lib"))
    .unwrap();
  let output = wkit(&example, &["package", "deb"]);
  let stderr = String::from_utf8(output.stderr).unwrap();
  assert!(!output.status.success());
  assert!(
    stderr.contains("help: set `package.deb.maintainer"),
    "{stderr}"
  );
}
