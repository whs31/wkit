use std::{collections::VecDeque, io::Write as _, path::PathBuf};

use super::{BuiltPackage, PackageRef};
use crate::{ProcessBuilder, Result, Shell, manifest::BuildType};

pub const TEST_PACKAGE: &str = "test_package";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
  Phase {
    status: &'static str,
    message: String,
  },
  Downloading(PackageRef),
  CompilingDependency(PackageRef),
  Configuring(PackageRef),
  Compiling(PackageRef),
  Testing(PackageRef),
  Step {
    current: u64,
    total: u64,
    target: String,
  },
  Diagnostic(String),
  Warning(String),
  BuildDir(PackageRef, PathBuf),
}

fn phase(status: &'static str, message: impl Into<String>) -> Event {
  Event::Phase {
    status,
    message: message.into(),
  }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct BuildLog {
  root: PackageRef,
  current: PackageRef,
  dependency: bool,
  building: bool,
  resolving: bool,
  compiling: bool,
  skip_next: bool,
  block: Option<Vec<String>>,
  in_error: bool,
  diagnostics: usize,
  errors: Vec<String>,
  tail: VecDeque<String>,
}

impl BuildLog {
  const TAIL: usize = 30;
  const NOISY_WARNINGS: [&str; 2] = ["experimental", "Workspace found"];

  pub fn new(root: PackageRef) -> Self {
    Self {
      current: root.clone(),
      root,
      dependency: false,
      building: false,
      resolving: false,
      compiling: false,
      skip_next: false,
      block: None,
      in_error: false,
      diagnostics: 0,
      errors: Vec::new(),
      tail: VecDeque::new(),
    }
  }

  pub fn set_root(&mut self, root: PackageRef) {
    self.current = root.clone();
    self.root = root;
    self.dependency = false;
    self.building = false;
  }

  pub fn start_build(&mut self) {
    self.building = true;
  }

  pub fn feed(&mut self, line: &str) -> Vec<Event> {
    let mut events = Vec::new();
    if let Some(block) = &mut self.block {
      if line.is_empty() || line.starts_with([' ', '\t']) {
        block.push(line.to_owned());
        return events;
      }
      events.extend(self.flush_block());
    }
    if self.in_error && line.starts_with([' ', '\t']) {
      if let Some(last) = self.errors.last_mut() {
        last.push_str("\n  ");
        last.push_str(line.trim());
      }
      return events;
    }
    self.in_error = false;
    if std::mem::take(&mut self.skip_next) {
      return events;
    }
    self.classify(line, &mut events);
    events
  }

  pub fn finish(&mut self) -> Vec<Event> {
    self.flush_block().into_iter().collect()
  }

  fn classify(&mut self, line: &str, events: &mut Vec<Event>) {
    let conan_line = lazy_regex::regex!(
      r"^(?:conanfile\.py(?: \((?<label>[^)]+)\))?|(?<reference>[\w.+-]+/[\w.+-]+(?:@[\w.+/-]+)?)(?<test> \(test package\))?): (?<rest>.*)$"
    );
    if let Some(error) = line.strip_prefix("ERROR: ") {
      self.errors.push(error.to_owned());
      self.in_error = true;
    } else if let Some(warning) = line.strip_prefix("WARN: ") {
      if !Self::NOISY_WARNINGS.iter().any(|n| warning.contains(n)) {
        events.push(Event::Warning(warning.to_owned()));
      }
    } else if ["CMake Warning", "CMake Error", "CMake Deprecation Warning"]
      .iter()
      .any(|p| line.starts_with(p))
    {
      self.block = Some(vec![line.to_owned()]);
    } else if line.starts_with("========") {
      self.building = false;
      if line.starts_with("======== Computing dependency graph") && !self.resolving {
        self.resolving = true;
        events.push(phase("Resolving", "dependencies"));
      } else if line.starts_with("======== Installing packages") {
        events.push(phase("Installing", "dependencies"));
      }
    } else if let Some((_, reference)) =
      lazy_regex::regex_captures!(r"^([\w.+-]+/[\w.+-]+(?:@[\w.+/-]+)?) export$", line)
    {
      self.set_root(PackageRef::parse(reference));
    } else if let Some((_, reference)) =
      lazy_regex::regex_captures!(r"^-------- Installing package (\S+) \(", line)
    {
      self.current = PackageRef::parse(reference);
    } else if let Some((_, reference)) =
      lazy_regex::regex_captures!(r"^Building from source ([^:\s]+)", line)
    {
      self.enter_dependency(PackageRef::parse(reference), events);
    } else if let Some(c) = conan_line.captures(line) {
      self.conan(&c, events);
    } else if let Some(dir) = line.strip_prefix("-- Build files have been written to: ") {
      if !self.dependency {
        events.push(Event::BuildDir(self.current.clone(), dir.trim().into()));
      }
    } else if line.starts_with("-- The C compiler identification")
      || line.starts_with("-- The CXX compiler identification")
    {
      if !self.dependency {
        events.push(Event::Configuring(self.current.clone()));
      }
    } else if let Some((_, current, total, description)) =
      lazy_regex::regex_captures!(r"^\[(\d+)/(\d+)\] (.*)$", line)
    {
      self.step(
        current.parse().unwrap_or_default(),
        total.parse().unwrap_or_default(),
        description,
        events,
      );
    } else if let Some((_, percent, description)) =
      lazy_regex::regex_captures!(r"^\[\s*(\d+)%\] (.*)$", line)
    {
      if !description.starts_with("Built target ") {
        self.step(
          percent.parse().unwrap_or_default(),
          100,
          description,
          events,
        );
      }
    } else if line.starts_with("FAILED: ") {
      self.skip_next = true;
    } else if !Self::is_chatter(line) && self.building {
      self.output(line, events);
    }
  }

  fn conan(&mut self, c: &regex::Captures<'_>, events: &mut Vec<Event>) {
    let package = c
      .name("label")
      .or_else(|| c.name("reference"))
      .map_or_else(|| self.root.clone(), |m| PackageRef::parse(m.as_str()));
    let rest = &c["rest"];
    if c.name("test").is_some() {
      self.test_package(rest, events);
    } else if rest.starts_with("Retrieving package") {
      events.push(Event::Downloading(package));
    } else if rest == "Building from source" {
      self.enter_dependency(package, events);
    } else if rest.starts_with("Package '") && rest.ends_with("' built") {
      self.dependency = false;
      self.building = false;
      self.current = self.root.clone();
    } else if rest == "Calling build()" && c.name("reference").is_none() {
      self.dependency = false;
      self.building = false;
      self.compiling = false;
      events.push(phase("Building", package.to_string()));
      self.current = package;
    } else if rest.starts_with("RUN: cmake --build") {
      self.building = true;
    }
  }

  fn enter_dependency(&mut self, package: PackageRef, events: &mut Vec<Event>) {
    self.building = false;
    self.compiling = false;
    self.tail.clear();
    self.current = package.clone();
    self.dependency = package.name != self.root.name;
    events.push(if self.dependency {
      Event::CompilingDependency(package)
    } else {
      phase("Building", package.to_string())
    });
  }

  fn test_package(&mut self, rest: &str, events: &mut Vec<Event>) {
    if rest == "Calling build()" {
      self.dependency = false;
      self.building = false;
      self.compiling = false;
      self.current = PackageRef::new(TEST_PACKAGE, None);
      events.push(phase("Building", TEST_PACKAGE));
    } else if rest.starts_with("RUN: cmake --build") {
      self.building = true;
    } else if rest == "Running test()" {
      self.building = true;
      events.push(Event::Testing(self.root.clone()));
    }
  }

  fn step(&mut self, current: u64, total: u64, description: &str, events: &mut Vec<Event>) {
    self.building = true;
    if !self.dependency && !self.compiling {
      self.compiling = true;
      events.push(Event::Compiling(self.current.clone()));
    }
    events.push(Event::Step {
      current,
      total,
      target: Self::target(description),
    });
  }

  fn target(description: &str) -> String {
    if let Some((_, dir, source)) = lazy_regex::regex_captures!(
      r"^Building \w+ object (?:(.*?)/)?CMakeFiles/[^/]+\.dir/(.+?)\.(?:o|obj)$",
      description
    ) {
      return if dir.is_empty() {
        source.to_owned()
      } else {
        format!("{dir}/{source}")
      };
    }
    if let Some((_, target)) = lazy_regex::regex_captures!(
      r"^Linking \w+ (?:[\w ]+ )?(?:library|executable) (.+)$",
      description
    ) {
      return target.to_owned();
    }
    description.to_owned()
  }

  fn output(&mut self, line: &str, events: &mut Vec<Event>) {
    if self.dependency {
      if self.tail.len() == Self::TAIL {
        self.tail.pop_front();
      }
      self.tail.push_back(line.to_owned());
    } else {
      self.diagnostics += 1;
      events.push(Event::Diagnostic(line.to_owned()));
    }
  }

  fn flush_block(&mut self) -> Option<Event> {
    let mut block = self.block.take()?;
    while block.last().is_some_and(|l| l.trim().is_empty()) {
      block.pop();
    }
    let text = block.join("\n");
    if block[0].starts_with("CMake Error") {
      self.diagnostics += 1;
      return Some(Event::Diagnostic(text));
    }
    (!self.dependency && !block[0].contains("(unused-cli)")).then_some(Event::Diagnostic(text))
  }

  fn is_chatter(line: &str) -> bool {
    line.trim().is_empty()
      || [
        "-- ",
        "ninja: ",
        "--------",
        ">>> ",
        "Command: ",
        "make[",
        "gmake[",
      ]
      .iter()
      .any(|p| line.starts_with(p))
      || (line.starts_with('*') && line.ends_with('*'))
  }

  fn failure(
    &self,
    process: &ProcessBuilder,
    status: std::process::ExitStatus,
    log: Option<&PathBuf>,
  ) -> anyhow::Error {
    let mut errors = Vec::new();
    if self.dependency {
      errors.extend(self.tail.iter().cloned());
      errors.extend(self.errors.iter().cloned());
    } else if self.diagnostics == 0 {
      errors.extend(self.errors.iter().cloned());
    }
    if errors.is_empty() && self.diagnostics == 0 {
      errors.push(format!("`{process}` failed ({status})"));
    }
    failed(&self.current, &errors, log.map(PathBuf::as_path))
  }
}

fn failed(package: &PackageRef, errors: &[String], log: Option<&std::path::Path>) -> anyhow::Error {
  let headline = if errors.is_empty() {
    format!("could not compile `{package}` due to previous errors")
  } else {
    format!("could not build `{package}`:\n  {}", errors.join("\n  "))
  };
  let note = log
    .map(|log| format!("\n\nnote: the full build log is in `{}`", log.display()))
    .unwrap_or_default();
  anyhow::anyhow!("{headline}{note}")
}

pub struct Reporter<'a> {
  shell: &'a mut Shell,
  log: BuildLog,
  build_type: BuildType,
  file: Option<(PathBuf, std::io::BufWriter<fs_err::File>)>,
  bar: Option<indicatif::ProgressBar>,
  stepping: bool,
  packages: Vec<BuiltPackage>,
}

impl<'a> Reporter<'a> {
  pub fn new(
    shell: &'a mut Shell,
    root: PackageRef,
    build_type: BuildType,
    log: Option<PathBuf>,
  ) -> Result<Self> {
    let file = match log {
      Some(path) => {
        if let Some(parent) = path.parent() {
          fs_err::create_dir_all(parent)?;
        }
        let file = std::io::BufWriter::new(fs_err::File::create(&path)?);
        Some((path, file))
      }
      None => None,
    };
    Ok(Self {
      shell,
      log: BuildLog::new(root),
      build_type,
      file,
      bar: None,
      stepping: false,
      packages: Vec::new(),
    })
  }

  pub fn set_root(&mut self, root: PackageRef) {
    self.log.set_root(root);
  }

  pub fn start_build(&mut self) {
    self.log.start_build();
  }

  pub fn run(&mut self, process: &ProcessBuilder) -> Result<()> {
    self.shell.verbose(|s| s.status("Running", process))?;
    if let Some((_, file)) = &mut self.file {
      writeln!(file, "$ {process}")?;
    }
    let mut failure = None;
    let status = process.stream(|line| {
      if let Err(e) = self.line(line) {
        failure.get_or_insert(e);
      }
    })?;
    for event in self.log.finish() {
      self.apply(event)?;
    }
    if let Some((_, file)) = &mut self.file {
      file.flush()?;
    }
    if let Some(e) = failure {
      return Err(e);
    }
    if !status.success() {
      self.clear();
      return Err(
        self
          .log
          .failure(process, status, self.file.as_ref().map(|(path, _)| path)),
      );
    }
    Ok(())
  }

  pub fn packages(&self) -> &[BuiltPackage] {
    &self.packages
  }

  pub fn finish(mut self) -> Vec<BuiltPackage> {
    self.clear();
    self.packages
  }

  fn line(&mut self, line: &str) -> Result<()> {
    if let Some((_, file)) = &mut self.file {
      writeln!(file, "{line}")?;
    }
    let verbose = self.shell.is_verbose();
    if verbose {
      self.shell.passthrough(line)?;
    }
    for event in self.log.feed(line) {
      if !verbose || matches!(event, Event::BuildDir(..)) {
        self.apply(event)?;
      }
    }
    Ok(())
  }

  fn apply(&mut self, event: Event) -> Result<()> {
    let build_type = self.build_type;
    match event {
      Event::Phase { status, message } => {
        let bar = self.shell.transient(status);
        bar.set_message(message);
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        self.bar = Some(bar);
        self.stepping = false;
      }
      Event::Downloading(package) => self.shell.status("Downloading", package)?,
      Event::CompilingDependency(package) => self.shell.status("Compiling", package)?,
      Event::Configuring(package) => self
        .shell
        .status("Configuring", format_args!("{package} ({build_type})"))?,
      Event::Compiling(package) => self
        .shell
        .status("Compiling", format_args!("{package} ({build_type})"))?,
      Event::Testing(package) => self.shell.status("Testing", package)?,
      Event::Step {
        current,
        total,
        target,
      } => {
        if !self.stepping {
          self.bar = Some(self.shell.progress("Building", total));
          self.stepping = true;
        }
        if let Some(bar) = &self.bar {
          bar.set_length(total);
          bar.set_position(current);
          bar.set_message(format!(": {target}"));
        }
      }
      Event::Diagnostic(text) => self.shell.passthrough(text)?,
      Event::Warning(text) => self.shell.warn(text)?,
      Event::BuildDir(package, build_dir) => {
        self.packages.retain(|p| p.package != package);
        self.packages.push(BuiltPackage { package, build_dir });
      }
    }
    Ok(())
  }

  fn clear(&mut self) {
    if let Some(bar) = self.bar.take() {
      bar.finish_and_clear();
    }
    self.stepping = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn translate(fixture: &str) -> (BuildLog, Vec<Event>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("tests/fixtures")
      .join(fixture);
    let text = std::fs::read_to_string(path).unwrap();
    let mut log = BuildLog::new(PackageRef::new("project", None));
    let mut events: Vec<_> = text.lines().flat_map(|line| log.feed(line)).collect();
    events.extend(log.finish());
    (log, events)
  }

  fn geodesy() -> PackageRef {
    PackageRef::parse("geodesy/2.4.1")
  }

  fn resolving() -> Event {
    phase("Resolving", "dependencies")
  }

  fn installing() -> Event {
    phase("Installing", "dependencies")
  }

  fn steps(events: &[Event]) -> Vec<String> {
    events
      .iter()
      .filter_map(|e| match e {
        Event::Step {
          current,
          total,
          target,
        } => Some(format!("{current}/{total} {target}")),
        _ => None,
      })
      .collect()
  }

  #[test]
  fn failures_point_at_the_log() {
    assert_eq!(
      failed(
        &PackageRef::parse("geodesy/2.4.1"),
        &[],
        Some("build/wkit-build.log".as_ref())
      )
      .to_string(),
      "could not compile `geodesy v2.4.1` due to previous errors\n\nnote: the full build log is in `build/wkit-build.log`"
    );
    assert_eq!(
      failed(
        &PackageRef::new("app", None),
        &["Package 'x/1.0' not resolved".into()],
        None
      )
      .to_string(),
      "could not build `app`:\n  Package 'x/1.0' not resolved"
    );
  }

  #[test]
  fn fresh_build_configures_then_compiles_the_root() {
    let (log, events) = translate("conan-fresh.log");
    let summary: Vec<_> = events
      .iter()
      .filter(|e| !matches!(e, Event::Step { .. }))
      .cloned()
      .collect();
    assert_eq!(
      summary,
      [
        resolving(),
        installing(),
        phase("Building", "geodesy v2.4.1"),
        Event::Configuring(geodesy()),
        Event::BuildDir(geodesy(), "/work/geodesy/build/Debug".into()),
        Event::Compiling(geodesy()),
      ]
    );
    assert_eq!(
      steps(&events),
      [
        "1/4 tests/test_geodesy.cpp",
        "2/4 src/geodesy.cpp",
        "3/4 libgeodesy.a",
        "4/4 tests/geodesy_tests"
      ]
    );
    assert_eq!(log.errors, Vec::<String>::new());
  }

  #[test]
  fn up_to_date_build_says_nothing_but_the_build_dir() {
    let (_, events) = translate("conan-noop.log");
    assert_eq!(
      events,
      [
        resolving(),
        installing(),
        phase("Building", "geodesy v2.4.1"),
        Event::BuildDir(geodesy(), "/work/geodesy/build/Debug".into())
      ]
    );
  }

  #[test]
  fn compiler_errors_pass_through_without_ninja_noise() {
    let (log, events) = translate("conan-error.log");
    let diagnostics: Vec<_> = events
      .iter()
      .filter_map(|e| match e {
        Event::Diagnostic(text) => Some(text.as_str()),
        _ => None,
      })
      .collect();
    assert_eq!(
      diagnostics,
      [
        "/work/geodesy/src/geodesy.cpp: In function ‘void broken()’:",
        "/work/geodesy/src/geodesy.cpp:42:17: error: ‘undefined_call’ was not declared in this scope",
        "   42 | void broken() { undefined_call(); }",
        "      |                 ^~~~~~~~~~~~~~",
      ]
    );
    assert_eq!(log.diagnostics, 4);
    assert_eq!(log.current, geodesy());
    assert!(!log.dependency);
    assert!(log.errors[0].starts_with("conanfile.py (geodesy/2.4.1): Error in build() method"));
  }

  #[test]
  fn dependencies_built_from_source_stay_quiet() {
    let (log, events) = translate("conan-dependency.log");
    assert_eq!(
      events[..3],
      [
        resolving(),
        installing(),
        Event::CompilingDependency(PackageRef::parse("zlib/1.3.1"))
      ]
    );
    assert_eq!(steps(&events).len(), 16);
    assert!(!events.iter().any(|e| matches!(
      e,
      Event::Diagnostic(_) | Event::BuildDir(..) | Event::Compiling(_)
    )));
    assert!(!log.dependency);
  }

  #[test]
  fn workspace_packages_report_their_own_build_dirs() {
    let (_, events) = translate("conan-workspace.log");
    let dirs: Vec<_> = events
      .iter()
      .filter_map(|e| match e {
        Event::BuildDir(package, dir) => Some(format!("{package} {}", dir.display())),
        _ => None,
      })
      .collect();
    assert_eq!(
      dirs,
      [
        "radar-core v0.5.0 /work/radar/core/build/Debug",
        "radar-app v0.5.0 /work/radar/app/build/Debug"
      ]
    );
    let noise: Vec<_> = events
      .iter()
      .filter(|e| matches!(e, Event::Diagnostic(_) | Event::Warning(_)))
      .collect();
    assert!(noise.is_empty(), "{noise:#?}");
    assert_eq!(events.iter().filter(|e| **e == resolving()).count(), 1);
  }

  #[test]
  fn resolution_failures_keep_conan_errors() {
    let (log, events) = translate("conan-missing.log");
    assert_eq!(events, [resolving()]);
    assert_eq!(
      log.errors,
      [
        "Package 'nonexistent/1.0' not resolved: Unable to find 'nonexistent/1.0' in remotes. Required by 'cli'"
      ]
    );
  }

  #[test]
  fn cmake_warnings_from_the_project_are_shown() {
    let mut log = BuildLog::new(PackageRef::new("crc8", None));
    let mut events = Vec::new();
    for line in [
      "CMake Warning at CMakeLists.txt:3 (message):",
      "  something odd",
      "",
      "-- Configuring done (0.0s)",
      "CMake Warning (unused-cli):",
      "  Manually-specified variables were not used by the project:",
      "",
      "-- Build files have been written to: /work/crc8/build/Debug",
    ] {
      events.extend(log.feed(line));
    }
    log.start_build();
    events.extend(log.feed("[1/1] Building C object CMakeFiles/crc8.dir/crc8.c.o"));
    events.extend(log.feed("crc8.c:3:5: warning: unused variable ‘x’"));
    let crc8 = PackageRef::new("crc8", None);
    assert_eq!(
      events,
      [
        Event::Diagnostic("CMake Warning at CMakeLists.txt:3 (message):\n  something odd".into()),
        Event::BuildDir(crc8.clone(), "/work/crc8/build/Debug".into()),
        Event::Compiling(crc8),
        Event::Step {
          current: 1,
          total: 1,
          target: "crc8.c".into()
        },
        Event::Diagnostic("crc8.c:3:5: warning: unused variable ‘x’".into()),
      ]
    );
  }

  #[test]
  fn create_treats_the_exported_recipe_as_the_root() {
    let mut log = BuildLog::new(PackageRef::new("project", None));
    let text = std::fs::read_to_string(
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conan-create.log"),
    )
    .unwrap();
    let events: Vec<_> = text.lines().flat_map(|line| log.feed(line)).collect();
    let summary: Vec<_> = events
      .iter()
      .filter(|e| {
        !matches!(
          e,
          Event::Step { .. } | Event::Phase { .. } | Event::BuildDir(..)
        )
      })
      .cloned()
      .collect();
    let test_package = PackageRef::new(TEST_PACKAGE, None);
    assert_eq!(
      summary,
      [
        Event::Downloading(PackageRef::parse("fmt/12.1.0")),
        Event::Downloading(PackageRef::parse("spdlog/1.17.0")),
        Event::Configuring(geodesy()),
        Event::Compiling(geodesy()),
        Event::Configuring(test_package.clone()),
        Event::Compiling(test_package),
        Event::Testing(geodesy()),
        Event::Diagnostic("111195".into()),
      ]
    );
  }

  #[test]
  fn make_progress_without_work_is_not_compiling() {
    let mut log = BuildLog::new(PackageRef::new("crc8", None));
    log.start_build();
    let events: Vec<_> = [
      "[ 33%] Built target crc8",
      "[ 66%] Built target crc8sum",
      "[100%] Built target crc8_tests",
    ]
    .into_iter()
    .flat_map(|line| log.feed(line))
    .collect();
    assert_eq!(events, []);
    let events = log.feed("[ 50%] Building C object CMakeFiles/crc8.dir/src/crc8.c.o");
    assert!(
      matches!(
        events[..],
        [Event::Compiling(_), Event::Step { current: 50, .. }]
      ),
      "{events:?}"
    );
  }
}
