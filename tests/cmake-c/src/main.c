#include <stdio.h>
#include <string.h>

#include "crc8.h"

int main(int argc, char** argv) {
  for (int i = 1; i < argc; ++i) {
    printf("%02x  %s\n", crc8((const uint8_t*)argv[i], strlen(argv[i])), argv[i]);
  }
  return 0;
}
