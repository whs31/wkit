find_package(Catch2 3 REQUIRED)

add_executable(geodesy_tests test_geodesy.cpp)
target_link_libraries(geodesy_tests PRIVATE geodesy::geodesy Catch2::Catch2WithMain)

include(Catch)
catch_discover_tests(geodesy_tests)
