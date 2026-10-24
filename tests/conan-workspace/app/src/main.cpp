#include <iostream>

#include <CLI/CLI.hpp>

#include "core/core.h"

int main(int argc, char** argv) {
  CLI::App app{"radar-app"};
  std::string name = "world";
  app.add_option("name", name, "Who to greet");
  CLI11_PARSE(app, argc, argv);
  std::cout << radar::greeting(name) << '\n';
  return 0;
}
