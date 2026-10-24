from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout


class FallbackPointsRecipe(ConanFile):
  name = "fallback-points"
  version = "3.1.0"
  package_type = "static-library"
  description = "Mock C++ library with Rust bindings for wkit tests"
  license = "MIT"
  settings = "os", "arch", "compiler", "build_type"
  options = {
    "test": [True, False],
  }
  default_options = {
    "test": False,
  }
  exports_sources = "CMakeLists.txt", "include/*", "src/*", "tests/*"

  def requirements(self):
    self.requires("nlohmann_json/3.12.0")
    self.requires("spdlog/1.17.0")

  def build_requirements(self):
    if self.options.test:
      self.test_requires("catch2/3.16.0")

  def layout(self):
    cmake_layout(self)

  def generate(self):
    CMakeDeps(self).generate()
    toolchain = CMakeToolchain(self)
    toolchain.cache_variables["BUILD_TESTING"] = bool(self.options.test)
    toolchain.generate()

  def build(self):
    cmake = CMake(self)
    cmake.configure()
    cmake.build()

  def package(self):
    CMake(self).install()

  def package_info(self):
    self.cpp_info.libs = ["fallback_points"]
