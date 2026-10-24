#include "geodesy/geodesy.h"

#include <cmath>
#include <numbers>

#include <spdlog/spdlog.h>

namespace geodesy {
namespace {

constexpr double kEarthRadius = 6'371'000.0;

double radians(double degrees) { return degrees * std::numbers::pi / 180.0; }

}  // namespace

double distance(const Point& from, const Point& to) {
  const double dlat = radians(to.latitude - from.latitude);
  const double dlon = radians(to.longitude - from.longitude);
  const double lat_term = std::pow(std::sin(dlat / 2), 2);
  const double lon_term = std::cos(radians(from.latitude)) * std::cos(radians(to.latitude)) *
                          std::pow(std::sin(dlon / 2), 2);
  const double a = lat_term + lon_term;
  const double meters = 2 * kEarthRadius * std::asin(std::sqrt(a));
  spdlog::debug("distance: {:.1f} m", meters);
  return meters;
}

double bearing(const Point& from, const Point& to) {
  const double dlon = radians(to.longitude - from.longitude);
  const double y = std::sin(dlon) * std::cos(radians(to.latitude));
  const double north = std::cos(radians(from.latitude)) * std::sin(radians(to.latitude));
  const double south =
      std::sin(radians(from.latitude)) * std::cos(radians(to.latitude)) * std::cos(dlon);
  const double degrees = std::atan2(y, north - south) * 180.0 / std::numbers::pi;
  return std::fmod(degrees + 360.0, 360.0);
}

}  // namespace geodesy
