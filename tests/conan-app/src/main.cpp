#include <iostream>

#include <CLI/CLI.hpp>

#include "config.h"

int main(int argc, char** argv) {
  CLI::App app{"groundlink " GROUNDLINK_VERSION " (" GROUNDLINK_DEPLOYMENT_SET ")"};
  std::string path = "config/dev.json";
  app.add_option("-c,--config", path, "Configuration file");
  CLI11_PARSE(app, argc, argv);

  const auto config = groundlink::load_config(path);
  std::cout << groundlink::describe(config) << '\n';
  return 0;
}
