cmake_minimum_required(VERSION 3.25)

project(radar_core VERSION 0.5.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(spdlog REQUIRED)

add_library(radar_core STATIC src/core.cpp)
add_library(radar::core ALIAS radar_core)
target_include_directories(radar_core PUBLIC
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include>
)
target_link_libraries(radar_core PUBLIC spdlog::spdlog)

install(TARGETS radar_core)
install(DIRECTORY include/ DESTINATION include)
