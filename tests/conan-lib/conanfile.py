from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout


class GeodesyRecipe(ConanFile):
  name = "geodesy"
  version = "2.4.1"
  package_type = "library"
  description = "Mock geodesy library for wkit tests"
  license = "MIT"
  settings = "os", "arch", "compiler", "build_type"
  options = {
    "shared": [True, False],
    "fPIC": [True, False],
    "test": [True, False],
  }
  default_options = {
    "shared": False,
    "fPIC": True,
    "test": False,
  }
  exports_sources = "CMakeLists.txt", "include/*", "src/*", "tests/*"

  def config_options(self):
    if self.settings.os == "Windows":
      del self.options.fPIC

  def configure(self):
    if self.options.shared:
      self.options.rm_safe("fPIC")

  def requirements(self):
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
    self.cpp_info.set_property("cmake_file_name", "geodesy")
    self.cpp_info.set_property("cmake_target_name", "geodesy::geodesy")
    self.cpp_info.libs = ["geodesy"]
