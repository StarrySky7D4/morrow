#ifdef NDEBUG
#error Consumer assertions must be enabled
#endif
#include <algorithm>
#include <utility>
#include <vector>
#include <cstdint>
#include "morrow_channel_v1.hpp"
#include <cassert>
#include <cstring>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>
static std::vector<uint8_t> load(const char *directory, const char *name) {
  std::ifstream file(std::string(directory) + "/" + name, std::ios::binary);
  assert(file);
  file.seekg(0, std::ios::end);
  std::streamoff length = file.tellg();
  assert(length > 0);
  size_t count = static_cast<size_t>(length);
  assert(count <= MP_MAX_CHANNEL_WIRE_BYTES);
  file.seekg(0);
  std::vector<uint8_t> bytes(count);
  file.read(reinterpret_cast<char *>(bytes.data()), static_cast<std::streamsize>(count));
  assert(file);
  return bytes;
}
static void verify_owned(const morrow::channel_v1::response &response,
                         const std::vector<uint8_t> &digest) {
  assert(response.code() == MP_CODEC_OK);
  mp_channel_response_view view{};
  assert(response.view(view) == MP_CODEC_OK);
  assert(view.status == MP_CHANNEL_FRAME && view.has_frame == 1);
  assert(view.sequence == 1 && view.bytes.length == 65536 && view.cursor.length == 4);
  assert(view.call_id.length == 32 && view.reference.length == 32 && view.source_epoch.length == 32);
  assert(digest.size() == 32 && view.frame_sha256.length == 32);
  assert(std::memcmp(view.frame_sha256.data, digest.data(), 32) == 0);
  for (uint32_t i = 0; i < 32; ++i) {
    assert(view.call_id.data[i] == 1 && view.reference.data[i] == 2 && view.source_epoch.data[i] == 3);
  }
  for (uint32_t i = 0; i < view.bytes.length; ++i) assert(view.bytes.data[i] == static_cast<uint8_t>(i % 251));
  const uint8_t cursor[4] = {0, 255, 1, 2};
  assert(std::memcmp(view.cursor.data, cursor, sizeof(cursor)) == 0);
}
int main(int argc, char **argv) {
  static_assert(sizeof(void *) == 8, "Windows x64 qualification only");
  static_assert(sizeof(mp_channel_host_v1) == 24, "Windows x64 HostV1 ABI");
  assert(argc == 2);
  const auto encoded_request = load(argv[1], "receive.request.capnp");
  auto encoded_reply = load(argv[1], "receive.response.capnp");
  const auto digest = load(argv[1], "frame.sha256");
  unsigned calls = 0;
  bool fail = false;
  auto callback = [&](const uint8_t *input, uint32_t length, uint8_t *output,
                      uint32_t capacity, uint32_t *written) -> uint32_t {
    assert(mp_channel_request_validate(input, length) == MP_CODEC_OK);
    assert(length == encoded_request.size() && std::memcmp(input, encoded_request.data(), length) == 0);
    assert(capacity == MP_MAX_CHANNEL_WIRE_BYTES && encoded_reply.size() <= capacity);
    ++calls;
    if (fail) throw std::runtime_error("synthetic consumer callback failure");
    std::memset(output, 0xa5, capacity);
    std::memcpy(output, encoded_reply.data(), encoded_reply.size());
    *written = static_cast<uint32_t>(encoded_reply.size());
    return 0;
  };
  auto host = morrow::channel_v1::local_adapter<decltype(callback)>::host(callback);
  morrow::channel_v1::request request;
  request.call_id.fill(1); request.reference.fill(2); request.source_epoch.fill(3);
  request.kind = MP_CHANNEL_RECEIVE; request.credit_bytes = 65536;
  auto first = morrow::channel_v1::response::call(host, request);
  assert(calls == 1); verify_owned(first, digest);
  auto second = morrow::channel_v1::response::call(host, request);
  assert(calls == 2); verify_owned(second, digest);
  auto owner = std::move(first); /* exercise response ownership transfer */
  std::fill(encoded_reply.begin(), encoded_reply.end(), uint8_t{0});
  verify_owned(owner, digest); verify_owned(second, digest);
  fail = true;
  auto failed = morrow::channel_v1::response::call(host, request);
  assert(failed.code() == MP_TRANSPORT_FAILURE && calls == 3);
  mp_channel_response_view view{};
  assert(failed.view(view) == MP_TRANSPORT_FAILURE);
  request.bytes.resize(MP_MAX_CHANNEL_PAYLOAD_BYTES + 1);
  assert(morrow::channel_v1::response::call(host, request).code() == MP_CODEC_LIMIT && calls == 3);
  encoded_reply.clear(); encoded_reply.shrink_to_fit();
  verify_owned(owner, digest); verify_owned(second, digest);
  std::cout << "COUNTS cpp successes=2 failures=1 calls=" << calls << " rejected=1\n";
  std::cout << "PASS standalone C++17: 2 successful owned64KiB replies, move ownership, exception/no retry, oversized rejection\n";
}
