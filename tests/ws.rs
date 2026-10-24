fn git(dir: &std::path::Path, args: &[&str]) {
  let output = std::process::Command::new("git")
    .current_dir(dir)
    .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
    .args(args)
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "git {args:?}: {}",
    String::from_utf8_lossy(&output.stderr)
  );
}

fn wkit(dir: &std::path::Path, args: &[&str]) -> (Option<i32>, String, String) {
  let output = std::process::Command::new(env!("CARGO_BIN_EXE_wkit"))
    .arg("-C")
    .arg(dir)
    .args(args)
    .env("WKIT_COLOR", "never")
    .output()
    .unwrap();
  (
    output.status.code(),
    String::from_utf8(output.stdout).unwrap(),
    String::from_utf8(output.stderr).unwrap(),
  )
}

#[test]
fn ws_clones_reports_and_runs_commands_across_projects() {
  let dir = tempfile::tempdir().unwrap();
  let remote = dir.path().join("remotes/tools/lint.git");
  let seed = dir.path().join("seed");
  std::fs::create_dir_all(&remote).unwrap();
  std::fs::create_dir_all(&seed).unwrap();
  git(&remote, &["init", "-q", "--bare", "-b", "main"]);
  git(&seed, &["init", "-q", "-b", "main"]);
  std::fs::write(seed.join("README"), "lint\n").unwrap();
  git(&seed, &["add", "."]);
  git(&seed, &["commit", "-qm", "init"]);
  git(&seed, &["push", "-q", remote.to_str().unwrap(), "main"]);

  let root = dir.path().join("ws");
  std::fs::create_dir_all(&root).unwrap();
  std::fs::write(
    root.join("workspace.toml"),
    format!(
      "version = 1\n\n[[project]]\npath = \"tools/lint\"\nurl = \"{}\"\nbranch = \"main\"\n",
      remote.display().to_string().replace('\\', "/")
    ),
  )
  .unwrap();

  let (code, stdout, _) = wkit(&root, &["ws", "status"]);
  assert_eq!(code, Some(1));
  assert!(stdout.contains("tools/lint  missing"), "{stdout}");

  let (code, _, stderr) = wkit(&root, &["ws", "init"]);
  assert_eq!(code, Some(0), "{stderr}");
  assert!(stderr.contains("Cloned tools/lint  main @"), "{stderr}");
  assert!(
    stderr.contains("Locked 1 of 1 projects changed in workspace.lock"),
    "{stderr}"
  );

  let (code, stdout, _) = wkit(&root.join("tools/lint"), &["ws", "status"]);
  assert_eq!(code, Some(0));
  assert!(stdout.ends_with("main  tools/lint\n"), "{stdout}");

  let (code, stdout, stderr) = wkit(&root, &["ws", "exec", "cat", "README"]);
  assert_eq!(code, Some(0), "{stderr}");
  assert_eq!(stdout, "lint\n");
  let (code, _, stderr) = wkit(&root, &["ws", "exec", "test -f MISSING"]);
  assert_eq!(code, Some(1));
  assert!(
    stderr.contains("1 of 1 projects failed: tools/lint"),
    "{stderr}"
  );

  let clone = root.join("tools/lint");
  git(&clone, &["config", "user.name", "t"]);
  git(&clone, &["config", "user.email", "t@example.com"]);
  std::fs::write(clone.join("README"), "lint, formatted\n").unwrap();
  let (code, _, stderr) = wkit(&root, &["ws", "commit", "fmt", "-j", "2"]);
  assert_eq!(code, Some(0), "{stderr}");
  assert!(
    stderr.contains("Committed tools/lint") && stderr.contains("(1 files)"),
    "{stderr}"
  );
  let log = std::process::Command::new("git")
    .current_dir(&clone)
    .args(["log", "-1", "--format=%s"])
    .output()
    .unwrap();
  assert_eq!(String::from_utf8_lossy(&log.stdout), "CHORE: Formatting\n");
  let (code, _, stderr) = wkit(&root, &["ws", "commit", "lock"]);
  assert_eq!(code, Some(0), "{stderr}");

  let (code, _, stderr) = wkit(dir.path(), &["ws", "status"]);
  assert_eq!(code, Some(2));
  assert!(stderr.contains("no `workspace.toml`"), "{stderr}");
}
