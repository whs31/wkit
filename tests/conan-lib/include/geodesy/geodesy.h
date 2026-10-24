#pragma once

namespace geodesy {

struct Point {
  double latitude;
  double longitude;
};

double distance(const Point& from, const Point& to);

double bearing(const Point& from, const Point& to);

}  // namespace geodesy
