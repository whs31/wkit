#pragma once

#include <cstddef>

extern "C" {

struct FallbackPoint {
  double latitude;
  double longitude;
  double altitude;
};

size_t fallback_points_parse(const char* json, FallbackPoint* out, size_t capacity);
}
