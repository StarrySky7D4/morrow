#include "morrow_plugin_codec.h"
#include <cstdint>
#include <vector>
extern "C" int32_t morrow_run() {
  std::vector<uint8_t> impossible(128u * 1024u * 1024u);
  // Escape storage to a separately compiled codec so allocation cannot be
  // elided.
  mp_request_v1 request{};
  uint32_t written = 0;
  return static_cast<int32_t>(
      mp_request_encode(&request, impossible.data(),
                        static_cast<uint32_t>(impossible.size()), &written));
}
