cmake_minimum_required(VERSION 3.25)

project(crc8 VERSION 0.3.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

add_library(crc8 src/crc8.c)
target_include_directories(crc8 PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)

add_executable(crc8sum src/main.c)
target_link_libraries(crc8sum PRIVATE crc8)

enable_testing()
add_executable(crc8_tests tests/test_crc8.c)
target_link_libraries(crc8_tests PRIVATE crc8)
add_test(NAME crc8_vectors COMMAND crc8_tests)

install(TARGETS crc8sum)
