use wkit_build::BuildOptions;
use wkit_manifest::Project;
use wkit_package::Packaging;

#[derive(Debug, Clone, Default, clap::Args)]
pub struct PackageSelection {
  /// Deployment set, passed to the Conan option in `package.set-option`
  #[arg(long, value_name = "SET")]
  set: Option<String>,

  /// Output directory [default: `package.output`, then build/package]
  #[arg(short, long, value_name = "DIR")]
  output: Option<std::path::PathBuf>,
}

impl PackageSelection {
  pub fn packaging(
    self,
    project: &Project,
    options: BuildOptions,
  ) -> wkit_package::Result<Packaging<'_>> {
    Packaging::new(project, options, self.set, self.output)
  }
}
