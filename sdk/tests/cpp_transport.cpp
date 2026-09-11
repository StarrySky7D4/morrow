#include "morrow_plugin_sdk.hpp"
#include <cassert>
#include <cstring>
#include <iostream>
extern "C" std::uint32_t exchange(void *ctx, const std::uint8_t *request,
                                  std::uint32_t length, std::uint8_t *output,
                                  std::uint32_t, std::uint32_t *written) {
  auto *calls = static_cast<int *>(ctx);
  ++*calls;
  std::memcpy(output, request, length);
  *written = length;
  return 0;
}
int main() {
  int calls = 0;
  mp_host_v1 host{1, sizeof(mp_host_v1), &calls, exchange};
  morrow::client client(host);
  std::uint8_t input[]{0, 255, 7};
  auto result = client.exchange(input, 3);
  assert(result.status == MP_OK &&
         result.bytes == std::vector<std::uint8_t>({0, 255, 7}) && calls == 1);
  auto invalid = client.exchange(input, MP_MAX_MESSAGE_BYTES + 1ull);
  assert(invalid.status == MP_LIMIT && invalid.bytes.empty() && calls == 1);
  std::cout << "PASS: C++ SDK uses C ABI and owned replies\n";
}
