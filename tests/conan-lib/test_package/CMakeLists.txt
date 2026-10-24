cmake_minimum_required(VERSION 3.25)

project(geodesy_test_package LANGUAGES CXX)

find_package(geodesy REQUIRED)

add_executable(example main.cpp)
target_link_libraries(example PRIVATE geodesy::geodesy)
target_compile_features(example PRIVATE cxx_std_20)
