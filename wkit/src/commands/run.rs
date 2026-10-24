use wkit_build::{Builder, Cargo};
use wkit_manifest::{Bin, BuildType, Project};

use super::build_selection::BuildSelection;
use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct RunArgs {
  /// The `[[bin]]` or executable to run [default: `project.default-run`]
  #[arg(long, value_name = "NAME")]
  bin: Option<String>,

  #[command(flatten)]
  selection: BuildSelection,

  /// Arguments passed to the program
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl RunArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = self.selection.clone().options();
    let builder = Builder::new(&project, &options);
    let bin = match &self.bin {
      Some(name) => project
        .manifest
        .bin(name)
        .cloned()
        .unwrap_or_else(|| Self::bare(name)),
      None => match project.manifest.default_bin() {
        Some(bin) => bin.clone(),
        None if !project.manifest.bins.is_empty() => anyhow::bail!(
          "several `[[bin]]` entries and no `project.default-run`; pass `--bin` with one of: {}",
          project
            .manifest
            .bins
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
        ),
        None => Self::bare(&project.name()),
      },
    };
    let native = bin.path.is_some() || (bin.package.is_none() && builder.builds_native());
    let process = if native {
      let outcome = builder.run_native(ctx.shell)?;
      let build_type = match &outcome {
        Some(outcome) => outcome.build_type,
        None => builder.build_type()?,
      };
      let exe = Self::executable(
        &project,
        &bin,
        build_type,
        outcome.as_ref().and_then(|o| o.build_dir()),
      )?;
      ctx.shell.status(
        "Running",
        format_args!(
          "`{}`",
          exe.strip_prefix(&project.root).unwrap_or(&exe).display()
        ),
      )?;
      wkit_core::ProcessBuilder::new(exe)
    } else if Cargo::applies(&project) {
      let build_type = options.build_type.unwrap_or(BuildType::Debug);
      let named = self.bin.is_some() || project.manifest.bin(&bin.name).is_some();
      Cargo::new(&project, &options).run_command(
        build_type,
        bin.package.as_deref(),
        named.then_some(bin.name.as_str()),
      )
    } else {
      anyhow::bail!("nothing to run: the project has no Conan, CMake or Cargo build");
    };
    let cwd = bin
      .cwd
      .as_ref()
      .map_or_else(|| ctx.cwd.clone(), |dir| project.join(dir));
    let mut process = process.cwd(cwd).args(&bin.args).args(&self.args);
    for (key, value) in &bin.env {
      process = process.env(key, value);
    }
    let status = process.status_unchecked()?;
    if status.success() {
      Ok(())
    } else {
      Err(Failure::silent(status.code().unwrap_or(1)).into())
    }
  }

  fn executable(
    project: &Project,
    bin: &Bin,
    build_type: BuildType,
    build_dir: Option<&std::path::Path>,
  ) -> anyhow::Result<std::path::PathBuf> {
    if let Some(path) = bin.resolve_path(build_type) {
      let path = project.join(path);
      anyhow::ensure!(
        path.is_file(),
        "`{}` does not exist after the build; check `path` in `[[bin]]`",
        path.display()
      );
      return Ok(path);
    }
    let dir = build_dir.ok_or_else(|| anyhow::anyhow!("the build reported no build directory"))?;
    wkit_core::fs::find_executable(dir, &bin.name).ok_or_else(|| {
      anyhow::anyhow!(
        "no executable named `{}` in `{}`; add a `[[bin]]` with a `path`",
        bin.name,
        dir.display()
      )
    })
  }

  fn bare(name: &str) -> Bin {
    Bin {
      name: name.to_owned(),
      path: None,
      package: None,
      args: Vec::new(),
      env: std::collections::BTreeMap::new(),
      cwd: None,
    }
  }
}
