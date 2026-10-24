#include <gtest/gtest.h>

#include "config.h"

TEST(Config, LoadsTheDevelopmentProfile) {
  const auto config = groundlink::load_config(FIXTURES "/dev.json");
  EXPECT_EQ(config.station, "bench");
  EXPECT_EQ(config.port, 14560);
}

TEST(Config, DescribesTheEndpoint) {
  EXPECT_EQ(groundlink::describe({"field", 1, true}), "field:1 (zlib)");
}

TEST(Config, MissingFilesThrow) {
  EXPECT_THROW(groundlink::load_config("missing.json"), std::runtime_error);
}
