#ifndef MORROW_PLUGIN_SDK_HPP
#define MORROW_PLUGIN_SDK_HPP
#include "morrow_plugin_sdk.h"
#include <cstddef>
#include <cstdint>
#include <vector>
namespace morrow {
struct reply {
  mp_status status;
  std::vector<std::uint8_t> bytes;
};
// C++17 convenience wrapper. STL objects never cross the C adapter boundary.
// Allocation exceptions occur before submission. Host lifetime is caller-owned.
class client {
  mp_host_v1 host_;

public:
  explicit client(mp_host_v1 host) : host_(host) {}
  client(const client &) = delete;
  client &operator=(const client &) = delete;
  reply exchange(const std::uint8_t *request, std::size_t length) {
    if (length > MP_MAX_MESSAGE_BYTES)
      return {MP_LIMIT, {}};
    reply result{MP_OK, std::vector<std::uint8_t>(MP_MAX_MESSAGE_BYTES)};
    std::uint32_t written = 0;
    result.status =
        mp_exchange(&host_, request, static_cast<std::uint32_t>(length),
                    result.bytes.data(), MP_MAX_MESSAGE_BYTES, &written);
    result.bytes.resize(result.status == MP_OK ? written : 0);
    return result;
  }
};
} // namespace morrow
#endif
