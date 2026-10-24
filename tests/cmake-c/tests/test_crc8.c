#include <stdio.h>

#include "crc8.h"

int main(void) {
  const uint8_t check[] = "123456789";
  const uint8_t got = crc8(check, sizeof check - 1);
  if (got != 0xF4) {
    fprintf(stderr, "crc8(\"123456789\") = %02x, expected f4\n", got);
    return 1;
  }
  return crc8(NULL, 0) == 0 ? 0 : 1;
}
