#include <array>

#include <catch2/catch_test_macros.hpp>

#include "points/points.h"

TEST_CASE("parses up to the given capacity") {
  std::array<FallbackPoint, 1> points{};
  const char* json = R"([{"lat": 1, "lon": 2, "alt": 3}, {"lat": 4}])";
  REQUIRE(fallback_points_parse(json, points.data(), points.size()) == 1);
  REQUIRE(points[0].altitude == 3.0);
}

TEST_CASE("rejects non-arrays") {
  std::array<FallbackPoint, 1> points{};
  REQUIRE(fallback_points_parse("{}", points.data(), points.size()) == 0);
}
