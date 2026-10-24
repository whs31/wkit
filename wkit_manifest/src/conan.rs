mod conan_sync;

pub use self::conan_sync::ConanSync;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanSection {
  pub sync: ConanSync,
}
