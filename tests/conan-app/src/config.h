#pragma once

#include <filesystem>
#include <string>

namespace groundlink {

struct Config {
  std::string station = "default";
  int port = 14550;
  bool compress = false;
};

Config load_config(const std::filesystem::path& path);

std::string describe(const Config& config);

}  // namespace groundlink
