use wkit_manifest::BuildType;

#[derive(Debug, Clone, Copy)]
pub struct ConanPresets;

impl ConanPresets {
  pub const USER_PRESETS: &str = "CMakeUserPresets.json";

  pub fn configure_preset(dir: &std::path::Path, build_type: BuildType) -> Option<String> {
    let user = Self::read(&dir.join(Self::USER_PRESETS))?;
    let mut fallback = None;
    for include in user["include"].as_array()?.iter().rev() {
      let Some(presets) = include.as_str().and_then(|i| Self::read(&dir.join(i))) else {
        continue;
      };
      for preset in presets["configurePresets"].as_array().into_iter().flatten() {
        let name = preset["name"].as_str().map(ToOwned::to_owned);
        match preset["cacheVariables"]["CMAKE_BUILD_TYPE"].as_str() {
          Some(t) if t == build_type.to_string() => return name,
          Some(_) => {}
          None => fallback = fallback.or(name),
        }
      }
    }
    fallback
  }

  fn read(path: &std::path::Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn picks_the_preset_for_the_build_type() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (folder, name, build_type) in [
      ("build/Debug", "conan-debug", Some("Debug")),
      ("build/Release", "conan-release", Some("Release")),
      ("build/msvc", "conan-default", None),
    ] {
      let generators = root.join(folder).join("generators");
      std::fs::create_dir_all(&generators).unwrap();
      let cache = build_type
        .map(|t| format!(r#", "cacheVariables": {{"CMAKE_BUILD_TYPE": "{t}"}}"#))
        .unwrap_or_default();
      std::fs::write(
        generators.join("CMakePresets.json"),
        format!(r#"{{"configurePresets": [{{"name": "{name}"{cache}}}]}}"#),
      )
      .unwrap();
    }
    std::fs::write(
      root.join(ConanPresets::USER_PRESETS),
      r#"{"include": ["build/Debug/generators/CMakePresets.json", "build/Release/generators/CMakePresets.json"]}"#,
    )
    .unwrap();
    assert_eq!(
      ConanPresets::configure_preset(root, BuildType::Release).as_deref(),
      Some("conan-release")
    );
    assert_eq!(
      ConanPresets::configure_preset(root, BuildType::MinSizeRel),
      None
    );
    std::fs::write(
      root.join(ConanPresets::USER_PRESETS),
      r#"{"include": ["build/msvc/generators/CMakePresets.json"]}"#,
    )
    .unwrap();
    assert_eq!(
      ConanPresets::configure_preset(root, BuildType::MinSizeRel).as_deref(),
      Some("conan-default")
    );
  }
}
