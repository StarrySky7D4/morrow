#include "morrow_channel_v1.hpp"
#include <cassert>
#include <iostream>
#include <stdexcept>
int main() {
  static_assert(sizeof(mp_channel_host_v1) == 24, "Windows x64 HostV1 ABI");
  unsigned calls = 0;
  auto callback = [&](const uint8_t *input, uint32_t length, uint8_t *, uint32_t capacity, uint32_t *) -> uint32_t {
    assert(mp_channel_request_validate(input, length) == MP_CODEC_OK);
    assert(capacity == MP_MAX_CHANNEL_WIRE_BYTES); ++calls;
    throw std::runtime_error("synthetic consumer callback failure");
  };
  auto host = morrow::channel_v1::local_adapter<decltype(callback)>::host(callback);
  morrow::channel_v1::request request;
  request.call_id.fill(1); request.reference.fill(2); request.source_epoch.fill(3);
  auto reply = morrow::channel_v1::response::call(host, request);
  assert(reply.code() == MP_TRANSPORT_FAILURE && calls == 1);
  mp_channel_response_view view{};
  assert(reply.view(view) == MP_TRANSPORT_FAILURE);
  request.bytes.resize(MP_MAX_CHANNEL_PAYLOAD_BYTES + 1);
  assert(morrow::channel_v1::response::call(host, request).code() == MP_CODEC_LIMIT && calls == 1);
  std::cout << "PASS standalone C++17 DLL/RAII and exception containment\n";
}
