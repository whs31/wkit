#include <fmt/format.h>

int main(int argc, char** argv) {
  fmt::print("flightlog {}: {} file(s)\n", FLIGHTLOG_VERSION, argc - 1);
  for (int i = 1; i < argc; ++i) {
    fmt::print("  {}\n", argv[i]);
  }
  return 0;
}
