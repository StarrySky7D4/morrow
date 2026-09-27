#include "morrow_plugin_mutation.hpp"
#include <algorithm>
#include <array>
#include <cassert>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

static std::vector<uint8_t> load(const std::string& dir, const char* name) {
  std::ifstream input(dir + "/" + name + ".bin", std::ios::binary);
  assert(input.good());
  std::vector<uint8_t> bytes{std::istreambuf_iterator<char>(input),
                             std::istreambuf_iterator<char>()};
  assert(!bytes.empty() && bytes.size() <= MP_MAX_MUTATION_FRAME_BYTES);
  return bytes;
}

int main(int argc, char** argv) {
  static_assert(!std::is_copy_constructible<morrow::mutation_response>::value, "response must own its handle");
  static_assert(std::is_nothrow_move_constructible<morrow::mutation_response>::value, "response move must not throw");
  static_assert(std::is_nothrow_move_assignable<morrow::mutation_response>::value, "response assignment must not throw");
  assert(argc == 2);
  const std::string dir(argv[1]);
  std::array<uint8_t, 32> reference{}, submission{};
  reference.fill(0x11); submission.fill(0x22);
  const std::array<uint8_t, 32> empty_hash = {
      0xe3,0xb0,0xc4,0x42,0x98,0xfc,0x1c,0x14,0x9a,0xfb,0xf4,0xc8,0x99,0x6f,0xb9,0x24,
      0x27,0xae,0x41,0xe4,0x64,0x9b,0x93,0x4c,0xa4,0x95,0x99,0x1b,0x78,0x52,0xb8,0x55};
  const std::array<uint8_t, 32> content_hash = {
      0x26,0xa6,0x6b,0x06,0x1e,0x8f,0x48,0xf3,0x99,0x27,0xc3,0x12,0xf2,0x52,0x93,0x95,
      0x97,0x29,0xee,0xe9,0x59,0x78,0xe2,0x89,0x2d,0x49,0xd3,0x51,0x2a,0x5c,0xc0,0x92};
  const std::vector<morrow::mutation_request> actions = {
      morrow::mutation_request::create(7, reference, submission, "mutation-vector-1", 30000, 0, empty_hash),
      morrow::mutation_request::remove(7, reference, submission, "mutation-vector-1", 30000),
      morrow::mutation_request::chunk(7, reference, submission, "mutation-vector-1", 30000, 0, {0,1,255}),
      morrow::mutation_request::commit(7, reference, submission, "mutation-vector-1", 30000),
      morrow::mutation_request::execute(7, reference, submission, "mutation-vector-1", 30000),
      morrow::mutation_request::query(7, reference, submission, "mutation-vector-1", 30000),
      morrow::mutation_request::cancel_plan(7, reference, submission, "mutation-vector-1", 30000),
      morrow::mutation_request::release(7, reference, submission, "mutation-vector-1", 30000),
  };
  for (const auto& action : actions) {
    const auto encoded = action.encode();
    assert(encoded.status == MP_CODEC_OK && !encoded.bytes.empty());
    assert(mp_mutation_request_validate(encoded.bytes.data(), static_cast<uint32_t>(encoded.bytes.size())) == MP_CODEC_OK);
  }
  const auto content = morrow::mutation_request::create(7, reference, submission,
      "mutation-vector-1", 30000, 3, content_hash).encode();
  assert(content.status == MP_CODEC_OK);
  assert(mp_mutation_request_validate(content.bytes.data(), static_cast<uint32_t>(content.bytes.size())) == MP_CODEC_OK);
  const auto encoded_execute = actions[4].encode();
  assert(encoded_execute.bytes == load(dir, "execute"));

  for (const char* name : {"create-empty", "create-content", "delete", "chunk", "commit",
                           "execute", "query", "cancel", "release"}) {
    const auto bytes = load(dir, name);
    assert(mp_mutation_request_validate(bytes.data(), static_cast<uint32_t>(bytes.size())) == MP_CODEC_OK);
  }
  for (const char* name : {"bad-zero-reference", "bad-empty-hash", "bad-chunk-overflow",
                           "bad-call", "bad-deadline", "bad-unicode-control", "bad-trailing"}) {
    const auto bytes = load(dir, name);
    assert(mp_mutation_request_validate(bytes.data(), static_cast<uint32_t>(bytes.size())) != MP_CODEC_OK);
  }
  for (const char* name : {"created", "unknown", "denied"}) {
    auto frame = load(dir, name);
    auto decoded = morrow::mutation_response::decode(encoded_execute, frame);
    assert(decoded.status() == MP_CODEC_OK);
    auto moved = std::move(decoded);
    assert(decoded.status() != MP_CODEC_OK && moved.status() == MP_CODEC_OK);
    morrow::mutation_response assigned;
    assigned = std::move(moved);
    assert(moved.status() != MP_CODEC_OK && assigned.status() == MP_CODEC_OK);
    auto* self = &assigned;
    assigned = std::move(*self);
    assert(assigned.status() == MP_CODEC_OK);
    frame.assign(frame.size(), 0);
    const auto view = assigned.view();
    assert(view.call_id == 7 && view.kind == MP_MUTATION_EXECUTE);
    assert(view.reference.length == 32 && view.reference.data[0] == 0x11);
    assert(view.submission.length == 32 && view.submission.data[0] == 0x22);
    assert(view.operation_id.length == std::string("mutation-vector-1").size());
    const auto original = load(dir, name);
    assert(view.encoded_frame.length == original.size());
    assert(std::equal(original.begin(), original.end(), view.encoded_frame.data));
    if (std::string(name) == "created") {
      assert(view.status == MP_MUTATION_STATUS_COMPLETED);
      assert(view.phase == MP_MUTATION_PHASE_OBSERVED && view.effect == MP_MUTATION_EFFECT_OS_SUCCEEDED);
    } else if (std::string(name) == "unknown") {
      assert(view.status == MP_MUTATION_STATUS_OUTCOME_UNKNOWN);
      assert(view.phase == MP_MUTATION_PHASE_OUTCOME_UNKNOWN && view.effect == MP_MUTATION_EFFECT_UNSPECIFIED);
    } else {
      assert(view.status == MP_MUTATION_STATUS_DENIED && view.phase == MP_MUTATION_PHASE_NONE);
    }
  }
  for (const char* name : {"bad-success-effect", "bad-denied-effect"}) {
    const auto decoded = morrow::mutation_response::decode(encoded_execute, load(dir, name));
    assert(decoded.status() != MP_CODEC_OK);
  }
  auto wrong = morrow::mutation_request::execute(8, reference, submission,
      "mutation-vector-1", 30000).encode();
  assert(morrow::mutation_response::decode(wrong, load(dir, "created")).status() == MP_CODEC_CORRELATION);
  auto foreign_reference = reference;
  foreign_reference[0] ^= 0x55;
  wrong = morrow::mutation_request::execute(7, foreign_reference, submission,
      "mutation-vector-1", 30000).encode();
  assert(morrow::mutation_response::decode(wrong, load(dir, "created")).status() == MP_CODEC_CORRELATION);
  auto foreign_submission = submission;
  foreign_submission[0] ^= 0x55;
  wrong = morrow::mutation_request::execute(7, reference, foreign_submission,
      "mutation-vector-1", 30000).encode();
  assert(morrow::mutation_response::decode(wrong, load(dir, "created")).status() == MP_CODEC_CORRELATION);
  wrong = morrow::mutation_request::execute(7, reference, submission,
      "different-operation", 30000).encode();
  assert(morrow::mutation_response::decode(wrong, load(dir, "created")).status() == MP_CODEC_CORRELATION);
  wrong = morrow::mutation_request::query(7, reference, submission,
      "mutation-vector-1", 30000).encode();
  assert(morrow::mutation_response::decode(wrong, load(dir, "created")).status() == MP_CODEC_CORRELATION);
  wrong = morrow::mutation_request::execute(7, reference, submission,
      "mutation-vector-1", 30000).encode();
  wrong.bytes.assign(MP_MAX_MUTATION_FRAME_BYTES + 1u, 0);
  assert(morrow::mutation_response::decode(wrong, load(dir, "created")).status() == MP_CODEC_LIMIT);
  assert(morrow::mutation_response::decode(encoded_execute,
      std::vector<uint8_t>(MP_MAX_MUTATION_FRAME_BYTES + 1u, 0)).status() == MP_CODEC_LIMIT);
  assert(morrow::mutation_request::create(7, reference, submission, "mutation-vector-1", 30000,
      MP_MAX_MUTATION_CONTENT_BYTES + 1u, content_hash).encode().status == MP_CODEC_LIMIT);
  assert(morrow::mutation_request::chunk(7, reference, submission, "mutation-vector-1", 30000,
      0, std::vector<uint8_t>(MP_MAX_MUTATION_CHUNK_BYTES + 1u, 1)).encode().status == MP_CODEC_LIMIT);
  assert(morrow::mutation_request::execute(7, reference, submission,
      std::string(MP_MAX_MUTATION_OPERATION_BYTES + 1u, 'x'), 30000).encode().status == MP_CODEC_LIMIT);
  std::cout << "PASS_SCOPED C++ mutation move-only facade, 21 vectors, eight actions and bounds\n";
}
