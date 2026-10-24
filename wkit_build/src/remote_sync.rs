use wkit_core::Shell;
use wkit_manifest::{BuildType, Project, conan::ConanSync};

use crate::{BuildOptions, Conan, Error, PackageRef, Result};

#[derive(Debug, Clone)]
pub struct RemoteSync<'a> {
  project: &'a Project,
  options: BuildOptions,
  config: ConanSync,
  dry_run: bool,
}

impl<'a> RemoteSync<'a> {
  pub fn new(
    project: &'a Project,
    options: &BuildOptions,
    remote: Option<String>,
    dry_run: bool,
  ) -> Self {
    let mut config = project.manifest.conan.sync.clone();
    config.remote = remote.or(config.remote);
    Self {
      project,
      options: BuildOptions {
        offline: true,
        ..options.clone()
      },
      config,
      dry_run,
    }
  }

  pub fn remote(&self) -> Option<&str> {
    self.config.remote.as_deref()
  }

  pub fn run(&self, shell: &mut Shell, build_type: BuildType) -> Result<usize> {
    let remote = self.config.remote.as_deref().ok_or_else(|| {
      Error::Message(
        "no remote to sync with: set `conan.sync.remote` or pass `--remote`".to_owned(),
      )
    })?;
    let conan = Conan::new(self.project, &self.options);
    Self::validate_remote(&conan, remote)?;

    let work = tempfile::tempdir()?;
    let graph_path = work.path().join("graph.json");
    let bar = shell.transient("Resolving");
    bar.set_message("dependencies from the local cache");
    let graph = Self::checked(
      &conan.graph_command(build_type, "json", &graph_path),
      "resolving the dependency graph",
    );
    bar.finish_and_clear();
    graph?;
    let graph = Self::read(&graph_path)?;
    Self::ensure_cached(&graph)?;

    let list_path = work.path().join("packages.json");
    Self::checked(
      &wkit_core::ProcessBuilder::new("conan")
        .cwd(&self.project.root)
        .args(["list", "--graph"])
        .arg(&graph_path)
        .args(["--format", "json", "--out-file"])
        .arg(&list_path),
      "listing the cached packages",
    )?;
    let Some(serde_json::Value::Object(mut cache)) = Self::read(&list_path)?
      .get_mut("Local Cache")
      .map(serde_json::Value::take)
    else {
      return Err(Error::Message(
        "`conan list` returned no local cache section".to_owned(),
      ));
    };
    if let Some(root) = Self::root_reference(&graph) {
      cache.remove(&root);
    }

    let action = if self.dry_run {
      "Checking"
    } else {
      "Uploading"
    };
    let bar = shell.progress(action, cache.len() as u64);
    for (index, (reference, details)) in cache.iter().enumerate() {
      let package = PackageRef::parse(reference);
      bar.set_message(format!(": {package}"));
      shell.status(action, &package)?;
      let list = work.path().join(format!("upload-{index}.json"));
      std::fs::write(
        &list,
        serde_json::json!({ "Local Cache": { reference: details } }).to_string(),
      )?;
      let mut upload = wkit_core::ProcessBuilder::new("conan")
        .cwd(&self.project.root)
        .args(["upload", "--list"])
        .arg(&list)
        .args(["--remote", remote, "--confirm"]);
      if self.config.check {
        upload = upload.arg("--check");
      }
      if self.config.force {
        upload = upload.arg("--force");
      }
      if self.dry_run {
        upload = upload.arg("--dry-run");
      }
      Self::checked(&upload, &format!("uploading `{package}`"))?;
      bar.inc(1);
    }
    bar.finish_and_clear();
    Ok(cache.len())
  }

  fn validate_remote(conan: &Conan<'_>, remote: &str) -> Result<()> {
    let remotes = conan.query(&["remote", "list"], &[], "could not list the Conan remotes")?;
    let found = remotes
      .as_array()
      .into_iter()
      .flatten()
      .find(|r| r["name"].as_str() == Some(remote))
      .ok_or_else(|| Error::Message(format!("Conan remote `{remote}` is not configured")))?;
    if found["enabled"].as_bool() == Some(false) {
      return Err(Error::Message(format!(
        "Conan remote `{remote}` is disabled"
      )));
    }
    Ok(())
  }

  pub fn ensure_cached(graph: &serde_json::Value) -> Result<()> {
    let mut missing: Vec<String> = graph["graph"]["nodes"]
      .as_object()
      .into_iter()
      .flat_map(serde_json::Map::values)
      .filter(|node| matches!(node["binary"].as_str(), Some("Missing" | "Invalid")))
      .map(|node| {
        let reference = PackageRef::parse(node["ref"].as_str().unwrap_or("unknown"));
        match node["package_id"].as_str() {
          Some(id) if !id.is_empty() => format!("{reference} ({id})"),
          _ => reference.to_string(),
        }
      })
      .collect();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
      Ok(())
    } else {
      Err(Error::Message(format!(
        "binaries missing from the local cache; run `wkit build` first:\n  {}",
        missing.join("\n  ")
      )))
    }
  }

  pub fn root_reference(graph: &serde_json::Value) -> Option<String> {
    let root = graph["graph"]["root"].as_object()?.keys().next()?.clone();
    let reference = graph["graph"]["nodes"][&root]["ref"].as_str()?;
    let reference = reference.split('#').next()?;
    (!reference.is_empty() && reference != "conanfile").then(|| reference.to_owned())
  }

  fn read(path: &std::path::Path) -> Result<serde_json::Value> {
    let text = wkit_core::fs::read_to_string(path)?;
    serde_json::from_str(&text)
      .map_err(|e| Error::Message(format!("could not parse `{}`: {e}", path.display())))
  }

  fn checked(process: &wkit_core::ProcessBuilder, what: &str) -> Result<()> {
    let output = process.output_unchecked()?;
    if output.status.success() {
      return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason: Vec<_> = stderr
      .lines()
      .filter_map(|l| l.strip_prefix("ERROR: "))
      .collect();
    Err(Error::Message(format!(
      "{what} failed: {}",
      reason.join("; ")
    )))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn graph_checks_find_the_root_and_missing_binaries() {
    let graph = serde_json::json!({
      "graph": {
        "root": { "0": "conanfile.py (geodesy/2.4.1)" },
        "nodes": {
          "0": { "ref": "geodesy/2.4.1", "binary": null },
          "1": { "ref": "fmt/12.1.0#abc", "binary": "Cache", "package_id": "44" },
          "2": { "ref": "zlib/1.3.1#def", "binary": "Missing", "package_id": "fc02" }
        }
      }
    });
    assert_eq!(
      RemoteSync::root_reference(&graph).as_deref(),
      Some("geodesy/2.4.1")
    );
    let err = RemoteSync::ensure_cached(&graph).unwrap_err().to_string();
    assert!(err.ends_with("\n  zlib v1.3.1 (fc02)"), "{err}");
  }
}
