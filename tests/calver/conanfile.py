from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout


class FlightlogRecipe(ConanFile):
  name = "flightlog"
  version = "2026.10.24.2"
  package_type = "application"
  description = "Mock calendar-versioned application for wkit tests"
  license = "MIT"
  settings = "os", "arch", "compiler", "build_type"
  exports_sources = "CMakeLists.txt", "src/*"

  def requirements(self):
    self.requires("fmt/12.1.0")

  def layout(self):
    cmake_layout(self)

  def generate(self):
    CMakeDeps(self).generate()
    CMakeToolchain(self).generate()

  def build(self):
    cmake = CMake(self)
    cmake.configure()
    cmake.build()

  def package(self):
    CMake(self).install()
