from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout


class GroundlinkRecipe(ConanFile):
  name = "groundlink"
  version = "1.8.0-rc.2"
  package_type = "application"
  description = "Mock ground station application for wkit tests"
  license = "MIT"
  settings = "os", "arch", "compiler", "build_type"
  options = {
    "test": [True, False],
    "with_compression": [True, False],
    "deployment_set": ["basic", "field"],
  }
  default_options = {
    "test": False,
    "with_compression": True,
    "deployment_set": "basic",
  }
  exports_sources = "CMakeLists.txt", "src/*", "tests/*"

  def requirements(self):
    self.requires("cli11/2.7.2")
    self.requires("nlohmann_json/3.12.0")
    if self.options.with_compression:
      self.requires("zlib/1.3.1")

  def build_requirements(self):
    if self.options.test:
      self.test_requires("gtest/1.18.0")

  def layout(self):
    cmake_layout(self)

  def generate(self):
    CMakeDeps(self).generate()
    toolchain = CMakeToolchain(self)
    toolchain.cache_variables["BUILD_TESTING"] = bool(self.options.test)
    toolchain.cache_variables["GROUNDLINK_WITH_COMPRESSION"] = bool(self.options.with_compression)
    toolchain.cache_variables["GROUNDLINK_DEPLOYMENT_SET"] = str(self.options.deployment_set)
    toolchain.generate()

  def build(self):
    cmake = CMake(self)
    cmake.configure()
    cmake.build()

  def package(self):
    CMake(self).install()
