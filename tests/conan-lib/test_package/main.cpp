#include <cstdio>

#include "geodesy/geodesy.h"

int main() {
  std::printf("%.0f\n", geodesy::distance({0.0, 0.0}, {0.0, 1.0}));
  return 0;
}
