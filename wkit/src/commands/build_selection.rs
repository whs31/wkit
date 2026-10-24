use wkit_build::BuildOptions;
use wkit_manifest::BuildType;

#[derive(Debug, Clone, Default, clap::Args)]
pub struct BuildSelection {
  /// Build in release mode, same as `--build-type Release`
  #[arg(short, long, conflicts_with = "build_type")]
  release: bool,

  /// Build type [default: from the Conan profile, else Debug]
  #[arg(long, value_name = "TYPE")]
  build_type: Option<BuildType>,

  /// Conan host profile [default: `build.conan.profile-host`, then `default`]
  #[arg(long, value_name = "NAME")]
  profile: Option<String>,

  /// Use only packages that are already available locally
  #[arg(long)]
  offline: bool,
}

impl BuildSelection {
  pub fn options(self) -> BuildOptions {
    BuildOptions {
      build_type: self
        .build_type
        .or(self.release.then_some(BuildType::Release)),
      profile: self.profile,
      offline: self.offline,
      ..BuildOptions::default()
    }
  }
}
