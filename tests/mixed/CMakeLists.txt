cmake_minimum_required(VERSION 3.25)

project(fallback_points
  VERSION 3.1.0
  LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include(CTest)

find_package(nlohmann_json REQUIRED)
find_package(spdlog REQUIRED)

add_library(fallback_points STATIC src/points.cpp)
target_include_directories(fallback_points PUBLIC
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include>
)
target_link_libraries(fallback_points PRIVATE nlohmann_json::nlohmann_json spdlog::spdlog)

if (BUILD_TESTING)
  find_package(Catch2 3 REQUIRED)
  add_executable(fallback_points_tests tests/test_points.cpp)
  target_link_libraries(fallback_points_tests PRIVATE fallback_points Catch2::Catch2WithMain)
  include(Catch)
  catch_discover_tests(fallback_points_tests)
endif ()

install(TARGETS fallback_points)
install(DIRECTORY include/ DESTINATION include)
