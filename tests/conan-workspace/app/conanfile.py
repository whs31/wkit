from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout


class RadarAppRecipe(ConanFile):
  name = "radar-app"
  version = "0.5.0"
  package_type = "application"
  description = "Mock workspace application for wkit tests"
  license = "MIT"
  settings = "os", "arch", "compiler", "build_type"
  exports_sources = "CMakeLists.txt", "src/*"

  def requirements(self):
    self.requires("radar-core/[>=0.5 <1]")
    self.requires("cli11/2.7.2")

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
