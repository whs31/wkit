pub(super) struct Patterns {
  pub conan_line: regex::Regex,
  pub export: regex::Regex,
  pub installing: regex::Regex,
  pub from_source: regex::Regex,
  pub ninja_step: regex::Regex,
  pub make_step: regex::Regex,
  pub object: regex::Regex,
  pub linking: regex::Regex,
}

impl Patterns {
  pub fn get() -> &'static Self {
    static PATTERNS: std::sync::LazyLock<Patterns> = std::sync::LazyLock::new(|| {
      Patterns {
      conan_line: regex::Regex::new(
        r"^(?:conanfile\.py(?: \((?<label>[^)]+)\))?|(?<reference>[\w.+-]+/[\w.+-]+(?:@[\w.+/-]+)?)(?<test> \(test package\))?): (?<rest>.*)$",
      )
      .expect("valid regex"),
      export: regex::Regex::new(r"^(?<reference>[\w.+-]+/[\w.+-]+(?:@[\w.+/-]+)?) export$")
        .expect("valid regex"),
      installing: regex::Regex::new(r"^-------- Installing package (?<reference>\S+) \(")
        .expect("valid regex"),
      from_source: regex::Regex::new(r"^Building from source (?<reference>[^:\s]+)")
        .expect("valid regex"),
      ninja_step: regex::Regex::new(r"^\[(?<current>\d+)/(?<total>\d+)\] (?<description>.*)$")
        .expect("valid regex"),
      make_step: regex::Regex::new(r"^\[\s*(?<percent>\d+)%\] (?<description>.*)$")
        .expect("valid regex"),
      object: regex::Regex::new(
        r"^Building \w+ object (?:(?<dir>.*?)/)?CMakeFiles/[^/]+\.dir/(?<source>.+?)\.(?:o|obj)$",
      )
      .expect("valid regex"),
      linking: regex::Regex::new(r"^Linking \w+ (?:[\w ]+ )?(?:library|executable) (?<target>.+)$")
        .expect("valid regex"),
    }
    });
    &PATTERNS
  }

  pub fn target(&self, description: &str) -> String {
    if let Some(c) = self.object.captures(description) {
      return match c.name("dir") {
        Some(dir) => format!("{}/{}", dir.as_str(), &c["source"]),
        None => c["source"].to_owned(),
      };
    }
    if let Some(c) = self.linking.captures(description) {
      return c["target"].to_owned();
    }
    description.to_owned()
  }
}
