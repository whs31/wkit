cmake_minimum_required(VERSION 3.25)

project(groundlink VERSION 1.8.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

option(GROUNDLINK_WITH_COMPRESSION "Compress telemetry dumps" ON)

include(CTest)

find_package(CLI11 REQUIRED)
find_package(nlohmann_json REQUIRED)

add_library(groundlink_core STATIC src/config.cpp)
target_include_directories(groundlink_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)
target_link_libraries(groundlink_core PUBLIC nlohmann_json::nlohmann_json)
target_compile_definitions(groundlink_core PUBLIC GROUNDLINK_VERSION="${PROJECT_VERSION}")

if (GROUNDLINK_WITH_COMPRESSION)
  find_package(ZLIB REQUIRED)
  target_link_libraries(groundlink_core PUBLIC ZLIB::ZLIB)
  target_compile_definitions(groundlink_core PUBLIC GROUNDLINK_WITH_COMPRESSION)
endif ()

add_executable(groundlink src/main.cpp)
target_link_libraries(groundlink PRIVATE groundlink_core CLI11::CLI11)

if (BUILD_TESTING)
  add_subdirectory(tests)
endif ()

install(TARGETS groundlink)
install(FILES src/plugin/assets/meta.lua DESTINATION share/groundlink/plugin)
