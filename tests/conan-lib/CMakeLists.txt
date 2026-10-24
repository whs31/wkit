cmake_minimum_required(VERSION 3.25)

project(geodesy
  VERSION 2.4.1
  DESCRIPTION "Mock geodesy library for wkit tests"
  LANGUAGES CXX
)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include(CTest)

find_package(spdlog REQUIRED)

add_library(geodesy src/geodesy.cpp)
add_library(geodesy::geodesy ALIAS geodesy)
target_include_directories(geodesy PUBLIC
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:include>
)
target_link_libraries(geodesy PRIVATE spdlog::spdlog)

if (BUILD_TESTING)
  add_subdirectory(tests)
endif ()

install(TARGETS geodesy)
install(DIRECTORY include/ DESTINATION include)
