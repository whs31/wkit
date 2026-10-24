find_package(GTest REQUIRED)

add_executable(groundlink_tests test_config.cpp)
target_link_libraries(groundlink_tests PRIVATE groundlink_core GTest::gtest_main)
target_compile_definitions(groundlink_tests PRIVATE FIXTURES="${PROJECT_SOURCE_DIR}/config")

include(GoogleTest)
gtest_discover_tests(groundlink_tests)
