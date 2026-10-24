#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_floating_point.hpp>

#include "geodesy/geodesy.h"

using Catch::Matchers::WithinAbs;

TEST_CASE("distance between identical points is zero") {
  const geodesy::Point point{55.75, 37.62};
  REQUIRE_THAT(geodesy::distance(point, point), WithinAbs(0.0, 1e-6));
}

TEST_CASE("bearing due east is ninety degrees") {
  REQUIRE_THAT(geodesy::bearing({0.0, 0.0}, {0.0, 1.0}), WithinAbs(90.0, 1e-6));
}

TEST_CASE("one degree of latitude is about 111 km", "[!benchmark]") {
  REQUIRE_THAT(geodesy::distance({0.0, 0.0}, {1.0, 0.0}), WithinAbs(111'195.0, 1.0));
}
