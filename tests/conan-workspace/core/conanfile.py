from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout


class RadarCoreRecipe(ConanFile):
  name = "radar-core"
  version = "0.5.0"
  package_type = "static-library"
  description = "Mock workspace library for wkit tests"
  license = "MIT"
  settings = "os", "arch", "compiler", "build_type"
  exports_sources = "CMakeLists.txt", "include/*", "src/*"

  def requirements(self):
    self.requires("spdlog/1.17.0", transitive_headers=True)

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

  def package_info(self):
    self.cpp_info.set_property("cmake_file_name", "radar-core")
    self.cpp_info.set_property("cmake_target_name", "radar::core")
    self.cpp_info.libs = ["radar_core"]
