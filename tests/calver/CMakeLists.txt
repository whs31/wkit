cmake_minimum_required(VERSION 3.25)

project(flightlog VERSION 2026.10.24.2 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(fmt REQUIRED)

add_executable(flightlog src/main.cpp)
target_link_libraries(flightlog PRIVATE fmt::fmt)
target_compile_definitions(flightlog PRIVATE FLIGHTLOG_VERSION="${PROJECT_VERSION}")

install(TARGETS flightlog)
