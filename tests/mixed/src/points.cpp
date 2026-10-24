#include "points/points.h"

#include <spdlog/spdlog.h>
#include <nlohmann/json.hpp>

extern "C" size_t fallback_points_parse(const char* json, FallbackPoint* out, size_t capacity) {
  const auto parsed = nlohmann::json::parse(json, nullptr, false);
  if (!parsed.is_array()) {
    spdlog::warn("fallback points must be a JSON array");
    return 0;
  }
  size_t count = 0;
  for (const auto& point : parsed) {
    if (count == capacity) {
      break;
    }
    out[count++] = {point.value("lat", 0.0), point.value("lon", 0.0), point.value("alt", 0.0)};
  }
  return count;
}
