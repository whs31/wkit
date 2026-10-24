#include "config.h"

#include <fstream>
#include <stdexcept>

#include <nlohmann/json.hpp>

namespace groundlink {

Config load_config(const std::filesystem::path& path) {
  std::ifstream file(path);
  if (!file) {
    throw std::runtime_error("cannot open " + path.string());
  }
  const auto json = nlohmann::json::parse(file);
  Config config;
  config.station = json.value("station", config.station);
  config.port = json.value("port", config.port);
  config.compress = json.value("compress", config.compress);
  return config;
}

std::string describe(const Config& config) {
  return config.station + ":" + std::to_string(config.port) + (config.compress ? " (zlib)" : "");
}

}  // namespace groundlink
