use wkit_build::{BuildLog, Event, PackageRef};

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

fn phase(status: &'static str, message: &str) -> Event {
  Event::Phase {
    status,
    message: message.to_owned(),
  }
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
  assert!(log.errors().is_empty());
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
  assert_eq!(log.diagnostics(), 4);
  assert_eq!(log.current(), &geodesy());
  assert!(!log.in_dependency());
  assert!(log.errors()[0].starts_with("conanfile.py (geodesy/2.4.1): Error in build() method"));
}

#[test]
fn dependencies_built_from_source_stay_quiet() {
  let (log, events) = translate("conan-dependency.log");
  let zlib = PackageRef::parse("zlib/1.3.1");
  assert_eq!(
    events[..3],
    [resolving(), installing(), Event::CompilingDependency(zlib)]
  );
  assert_eq!(steps(&events).len(), 16);
  assert!(!events.iter().any(|e| matches!(
    e,
    Event::Diagnostic(_) | Event::BuildDir(..) | Event::Compiling(_)
  )));
  assert!(!log.in_dependency());
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
    log.errors(),
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
  let test_package = PackageRef::new(BuildLog::TEST_PACKAGE, None);
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
