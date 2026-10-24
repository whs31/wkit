#include "core/core.h"

#include <spdlog/spdlog.h>

namespace radar {

std::string greeting(const std::string& name) {
  spdlog::debug("greeting {}", name);
  return "hello, " + name;
}

}  // namespace radar
