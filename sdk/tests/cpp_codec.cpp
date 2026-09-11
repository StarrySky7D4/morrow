#include "morrow_plugin_codec.hpp"
#include <cassert>
#include <fstream>
#include <iostream>
#include <iterator>
#include <type_traits>
std::vector<uint8_t> fixture(const std::string &directory,
                             const std::string &name) {
  std::ifstream in(directory + "/" + name + ".capnp", std::ios::binary);
  assert(in);
  return {std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>()};
}
int main(int argc, char **argv) {
  assert(argc == 2);
  const std::string dir = argv[1];
  static_assert(!std::is_copy_constructible_v<morrow::decoded_reply>);
  static_assert(std::is_nothrow_move_constructible_v<morrow::decoded_reply>);
  auto rename = morrow::request::rename("vector-op", "legacy-123",
                                        UINT64_MAX - 1, u8"消息 🪷");
  // Request ownership survives copy/move; no spans into a moved small string.
  auto copy = rename;
  auto moved = std::move(copy);
  auto summary = morrow::request::summary("vector-op", "legacy-123");
  auto query = morrow::request::query("vector-op", "legacy-123", "vector-op");
  auto attachment = morrow::request::attachment("vector-op", "legacy-123",
                                                "asset-1", UINT64_MAX, 2, 3);
  for (const auto &entry : std::vector<std::pair<morrow::request, std::string>>{
           {moved, "rename"},
           {summary, "summary"},
           {query, "query"},
           {attachment, "attachment"}}) {
    auto encoded = entry.first.encode();
    assert(encoded.status == MP_CODEC_OK);
    assert(encoded.bytes == fixture(dir, entry.second + "-request"));
  }
  auto bytes = fixture(dir, "renamed-reply");
  auto reply = morrow::decoded_reply::decode(rename, bytes);
  assert(reply.status() == MP_CODEC_OK);
  // Decoder owns the result independently of incoming message memory.
  bytes.assign(bytes.size(), 0);
  auto owned = std::move(reply);
  assert(reply.status() != MP_CODEC_OK);
  auto view = owned.view();
  assert(view.kind == MP_REPLY_RENAMED && view.revision == UINT64_MAX &&
         view.sha256.length == 32 && view.sha256.data[0] == 42);
  auto other =
      morrow::decoded_reply::decode(summary, fixture(dir, "summary-reply"));
  assert(other.view().format_version == 1 &&
         other.view().revision == UINT64_MAX);
  assert(std::string(reinterpret_cast<const char *>(other.view().title.data),
                     other.view().title.length) == u8"消息 🪷");
  other = std::move(owned);
  assert(other.view().kind == MP_REPLY_RENAMED);
  auto denied =
      morrow::decoded_reply::decode(rename, fixture(dir, "denied-reply"));
  assert(denied.view().kind == MP_REPLY_REJECTED && denied.view().failure == 0);
  auto absent =
      morrow::decoded_reply::decode(query, fixture(dir, "absent-reply"));
  assert(absent.view().kind == MP_REPLY_QUERY && absent.view().state == 0);
  auto committed =
      morrow::decoded_reply::decode(query, fixture(dir, "committed-reply"));
  assert(committed.view().state == 1 &&
         committed.view().revision == UINT64_MAX);
  auto part = morrow::decoded_reply::decode(attachment,
                                            fixture(dir, "attachment-reply"));
  view = part.view();
  assert(view.kind == MP_REPLY_ATTACHMENT && view.offset == 2 &&
         view.total_length == 5 && view.bytes.length == 3 &&
         view.bytes.data[1] == 255);
  auto wrong =
      morrow::decoded_reply::decode(rename, fixture(dir, "summary-reply"));
  assert(wrong.status() == MP_CODEC_CORRELATION);
  bool threw = false;
  try {
    wrong.view();
  } catch (const std::logic_error &) {
    threw = true;
  }
  assert(threw);
  assert(morrow::request::rename("bad/id", "legacy-123", 1, "title")
             .encode()
             .status == MP_CODEC_INVALID);
  // Exercise raw C invalid-input paths and output initialization guarantees.
  mp_request_v1 raw{};
  raw.abi_version = 1;
  raw.struct_size = sizeof(raw);
  raw.kind = MP_REQUEST_SUMMARY;
  raw.request_id = {reinterpret_cast<const uint8_t *>("vector-op"), 9};
  raw.card_id = {reinterpret_cast<const uint8_t *>("legacy-123"), 10};
  uint8_t output[65536];
  uint32_t length = 42;
  assert(mp_request_encode(&raw, output, 1, &length) == MP_CODEC_LIMIT &&
         length == 0);
  raw.abi_version = 2;
  assert(mp_request_encode(&raw, output, sizeof(output), &length) ==
             MP_CODEC_CONTRACT &&
         length == 0);
  raw.abi_version = 1;
  const uint8_t invalid[] = {255};
  raw.card_id = {invalid, 1};
  assert(mp_request_encode(&raw, output, sizeof(output), &length) ==
         MP_CODEC_INVALID);
  raw.card_id = {nullptr, 1};
  assert(mp_request_encode(&raw, output, sizeof(output), &length) ==
         MP_CODEC_INVALID);
  raw.card_id = {reinterpret_cast<const uint8_t *>("legacy-123"), 10};
  bytes = fixture(dir, "summary-reply");
  mp_reply *handle = nullptr;
  assert(mp_reply_decode(bytes.data(), static_cast<uint32_t>(bytes.size()),
                         &raw, &handle) == MP_CODEC_OK &&
         handle);
  assert(mp_reply_get(handle, &view, 1) == MP_CODEC_LIMIT);
  mp_reply_free(handle);
  handle = nullptr;
  assert(mp_reply_decode(nullptr, 1, &raw, &handle) == MP_CODEC_INVALID &&
         handle == nullptr);
  mp_reply_free(nullptr);
  std::cout << "PASS: C codec ABI and C++ owned requests/replies, four "
               "encoders, six replies, UInt64 and rejection paths"
            << std::endl;
}
