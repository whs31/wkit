mod event;
mod patterns;

pub use self::event::Event;
use self::patterns::Patterns;
use crate::PackageRef;

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
  tail: std::collections::VecDeque<String>,
}

impl BuildLog {
  pub const TAIL: usize = 30;
  pub const TEST_PACKAGE: &str = "test_package";
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
      tail: std::collections::VecDeque::new(),
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

  pub fn current(&self) -> &PackageRef {
    &self.current
  }

  pub fn in_dependency(&self) -> bool {
    self.dependency
  }

  pub fn diagnostics(&self) -> usize {
    self.diagnostics
  }

  pub fn errors(&self) -> &[String] {
    &self.errors
  }

  pub fn tail(&self) -> impl Iterator<Item = &str> {
    self.tail.iter().map(String::as_str)
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
    if self.skip_next {
      self.skip_next = false;
      return events;
    }
    self.classify(line, &mut events);
    events
  }

  pub fn finish(&mut self) -> Vec<Event> {
    self.flush_block().into_iter().collect()
  }

  fn classify(&mut self, line: &str, events: &mut Vec<Event>) {
    let patterns = Patterns::get();
    if let Some(error) = line.strip_prefix("ERROR: ") {
      self.errors.push(error.to_owned());
      self.in_error = true;
    } else if let Some(warning) = line.strip_prefix("WARN: ") {
      if !Self::NOISY_WARNINGS.iter().any(|n| warning.contains(n)) {
        events.push(Event::Warning(warning.to_owned()));
      }
    } else if line.starts_with("CMake Warning")
      || line.starts_with("CMake Error")
      || line.starts_with("CMake Deprecation Warning")
    {
      self.block = Some(vec![line.to_owned()]);
    } else if line.starts_with("========") {
      self.building = false;
      if line.starts_with("======== Computing dependency graph") && !self.resolving {
        self.resolving = true;
        events.push(Self::phase("Resolving", "dependencies"));
      } else if line.starts_with("======== Installing packages") {
        events.push(Self::phase("Installing", "dependencies"));
      }
    } else if let Some(c) = patterns.export.captures(line) {
      self.set_root(PackageRef::parse(&c["reference"]));
    } else if let Some(c) = patterns.installing.captures(line) {
      self.current = PackageRef::parse(&c["reference"]);
    } else if let Some(c) = patterns.from_source.captures(line) {
      self.enter_dependency(PackageRef::parse(&c["reference"]), events);
    } else if let Some(c) = patterns.conan_line.captures(line) {
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
    } else if let Some(c) = patterns.ninja_step.captures(line) {
      let current = c["current"].parse().unwrap_or_default();
      let total = c["total"].parse().unwrap_or_default();
      self.step(current, total, patterns.target(&c["description"]), events);
    } else if let Some(c) = patterns.make_step.captures(line) {
      let percent = c["percent"].parse().unwrap_or_default();
      self.step(percent, 100, patterns.target(&c["description"]), events);
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
      events.push(Self::phase("Building", &package.to_string()));
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
    if self.dependency {
      events.push(Event::CompilingDependency(package));
    } else {
      events.push(Self::phase("Building", &package.to_string()));
    }
  }

  fn test_package(&mut self, rest: &str, events: &mut Vec<Event>) {
    if rest == "Calling build()" {
      self.dependency = false;
      self.building = false;
      self.compiling = false;
      self.current = PackageRef::new(Self::TEST_PACKAGE, None);
      events.push(Self::phase("Building", Self::TEST_PACKAGE));
    } else if rest.starts_with("RUN: cmake --build") {
      self.building = true;
    } else if rest == "Running test()" {
      self.building = true;
      events.push(Event::Testing(self.root.clone()));
    }
  }

  fn step(&mut self, current: u64, total: u64, target: String, events: &mut Vec<Event>) {
    self.building = true;
    if !self.dependency && !self.compiling {
      self.compiling = true;
      events.push(Event::Compiling(self.current.clone()));
    }
    events.push(Event::Step {
      current,
      total,
      target,
    });
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
    let unused_cli = block[0].contains("(unused-cli)");
    (!self.dependency && !unused_cli).then_some(Event::Diagnostic(text))
  }

  fn phase(status: &'static str, message: &str) -> Event {
    Event::Phase {
      status,
      message: message.to_owned(),
    }
  }

  fn is_chatter(line: &str) -> bool {
    line.trim().is_empty()
      || line.starts_with("-- ")
      || line.starts_with("ninja: ")
      || line.starts_with("--------")
      || line.starts_with(">>> ")
      || line.starts_with("Command: ")
      || (line.starts_with('*') && line.ends_with('*'))
      || line.starts_with("make[")
      || line.starts_with("gmake[")
  }
}
