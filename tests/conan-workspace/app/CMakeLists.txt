cmake_minimum_required(VERSION 3.25)

project(radar_app VERSION 0.5.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(radar-core REQUIRED)
find_package(CLI11 REQUIRED)

add_executable(radar-app src/main.cpp)
target_link_libraries(radar-app PRIVATE radar::core CLI11::CLI11)

install(TARGETS radar-app)
