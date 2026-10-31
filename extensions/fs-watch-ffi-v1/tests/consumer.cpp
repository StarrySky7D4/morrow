#include "morrow_fs_watch_v1.hpp"
#include <algorithm>
#include <fstream>
#include <iostream>
#include <iterator>
#include <type_traits>

namespace watch = morrow::fs_watch_v1;
static_assert(!std::is_copy_constructible_v<watch::frame> &&
              !std::is_copy_assignable_v<watch::frame>);
static_assert(!std::is_copy_constructible_v<watch::state> &&
              !std::is_copy_assignable_v<watch::state>);
static_assert(std::is_nothrow_move_constructible_v<watch::frame> &&
              std::is_nothrow_move_assignable_v<watch::frame>);
static_assert(std::is_nothrow_move_constructible_v<watch::state> &&
              std::is_nothrow_move_assignable_v<watch::state>);
static unsigned checks = 0, scenarios = 0;
static void require(bool value, const char *reason) {
  ++checks;
  if (!value)
    throw std::runtime_error(reason);
}
static void passed(const char *name) {
  ++scenarios;
  std::cout << "CPP_SCENARIO " << name << " PASS\n";
}
template <class Operation> void refused(uint32_t code, Operation operation) {
  try {
    operation();
  } catch (const watch::error &error) {
    require(error.code() == code, "wrong rejection status");
    return;
  }
  throw std::runtime_error("expected rejection missing");
}
static std::vector<uint8_t> read(const std::string &path) {
  std::ifstream file(path, std::ios::binary);
  require(file.good(), "fixture unreadable");
  std::vector<uint8_t> result((std::istreambuf_iterator<char>(file)),
                              std::istreambuf_iterator<char>());
  require(!file.bad() && result.size() >= 16 &&
              result.size() <= MFW_MAX_WIRE_BYTES,
          "fixture size or read failed");
  return result;
}
static watch::digest epoch(uint8_t value) {
  watch::digest result{};
  result.fill(value);
  return result;
}
static std::vector<uint8_t> units(const std::u16string &name) {
  std::vector<uint8_t> result;
  for (char16_t unit : name) {
    result.push_back(static_cast<uint8_t>(unit));
    result.push_back(static_cast<uint8_t>(unit >> 8));
  }
  return result;
}
int main(int argc, char **argv) try {
  require(argc == 8, "six originals and one synthetic directory required");
  std::array<std::vector<uint8_t>, 6> original;
  for (size_t i = 0; i < original.size(); i++)
    original[i] = read(argv[i + 1]);
  const std::string directory = argv[7];
  const auto terminal = read(directory + "/after-events.capnp");
  const auto gap = read(directory + "/terminal-2.capnp");
  const auto unpaired = read(directory + "/unpaired.capnp");
  const auto old_name = units(u"\u96ea \U0001f600.txt");
  const auto new_name = units(u"renamed \U0001f600.txt");
  const auto pending_name = units(u"pending.txt");
  uint32_t mask = 0;
  for (size_t i = 0; i < original.size(); i++) {
    auto frame = watch::frame::decode(original[i]);
    const auto info = frame.info();
    require(info.sequence == (i < 5 ? i + 1 : 1) &&
                info.changes == (i == 3 ? 2u : 1u) && info.end == MFW_CONTINUE,
            "original observation metadata");
    require(std::equal(std::begin(info.epoch), std::end(info.epoch),
                       epoch(0x42).begin()),
            "original epoch");
    require(frame.encode() == original[i], "native payload byte roundtrip");
    for (uint32_t j = 0; j < info.changes; j++) {
      const auto view = frame.change(j);
      const auto &expected =
          i == 5
              ? pending_name
              : ((view.kind == MFW_REMOVED || view.kind == MFW_RENAME_NEW_NAME)
                     ? new_name
                     : old_name);
      require(view.kind >= 1 && view.kind <= 5 &&
                  view.name_bytes == expected.size(),
              "raw component length");
      require(std::equal(expected.begin(), expected.end(), view.name),
              "raw component units");
      if (i < 5)
        mask |= 1u << view.kind;
    }
  }
  require(mask == 62, "all native event kinds observed");
  passed("six-unchanged-native-payloads-and-raw-components");
  {
    watch::state state(epoch(0x42), 10, 100, 65536);
    uint64_t bytes = 0;
    uint32_t changes = 0;
    for (size_t i = 0; i < 5; i++) {
      auto frame = state.receive(original[i]);
      const auto sequence = frame.info().sequence;
      bytes += original[i].size();
      changes += frame.info().changes;
      const auto digest = watch::hash(original[i]);
      auto wrong = digest;
      wrong[0] ^= 1;
      refused(MFW_PENDING, [&] { (void)state.receive(original[i]); });
      refused(MFW_CORRELATION,
              [&] { state.acknowledge(sequence + 1, digest); });
      refused(MFW_CORRELATION, [&] { state.acknowledge(sequence, wrong); });
      require(state.snapshot().phase == MFW_PENDING_PHASE,
              "wrong ACK advanced pending state");
      state.acknowledge(sequence, digest);
      require(state.snapshot().phase == MFW_READY_PHASE, "correct ACK failed");
      refused(MFW_STATE, [&] { state.acknowledge(sequence, digest); });
    }
    auto end = state.receive(terminal);
    require(end.info().sequence == 6 && end.info().end == MFW_WINDOW_ENDED,
            "synthetic sequence six terminal");
    state.acknowledge(6, watch::hash(terminal));
    state.transport_lost();
    const auto v = state.snapshot();
    require(v.phase == MFW_ENDED_PHASE && v.end == MFW_WINDOW_ENDED &&
                v.frames == 6 && v.changes == changes &&
                v.wire_bytes == bytes + terminal.size(),
            "historical terminal or charges drifted");
    refused(MFW_STATE, [&] { (void)state.receive(original[0]); });
    passed("original-digest-ack-and-synthetic-terminal");
  }
  {
    auto first = watch::frame::decode(original[0]);
    auto moved = std::move(first);
    refused(MFW_INVALID, [&] { (void)first.info(); });
    require(moved.encode() == original[0], "move constructor changed owner");
    auto replacement = watch::frame::decode(original[1]);
    replacement = std::move(moved);
    refused(MFW_INVALID, [&] { (void)moved.info(); });
    require(replacement.encode() == original[0],
            "move assignment changed owner");
    watch::state initial(epoch(0x42), 10, 100, 65536);
    auto pending = initial.receive(original[5]);
    watch::state moved_state(std::move(initial));
    refused(MFW_INVALID, [&] { (void)initial.snapshot(); });
    require(moved_state.snapshot().frames == 1, "state move lost charge");
    watch::state replacement_state(epoch(0x43), 2, 0, 65536);
    replacement_state = std::move(moved_state);
    refused(MFW_INVALID, [&] { (void)moved_state.snapshot(); });
    replacement_state.transport_lost();
    require(replacement_state.snapshot().phase == MFW_UNKNOWN_PHASE &&
                replacement_state.snapshot().frames == 1 &&
                replacement_state.snapshot().wire_bytes == original[5].size(),
            "moved pending state refunded loss");
    refused(MFW_UNKNOWN, [&] {
      replacement_state.acknowledge(pending.info().sequence,
                                    watch::hash(original[5]));
    });
    passed("raii-moves-preserve-owners-and-unacknowledged-charge");
  }
  {
    watch::state state(epoch(0x42), 1, 100, 65536);
    auto first = state.receive(original[0]);
    state.acknowledge(first.info().sequence, watch::hash(original[0]));
    refused(MFW_BUDGET, [&] { (void)state.receive(original[1]); });
    require(state.snapshot().phase == MFW_UNKNOWN_PHASE &&
                state.snapshot().frames == 1,
            "budget exhaustion refunded or reset");
    auto malformed = original[0];
    malformed.pop_back();
    refused(MFW_LIMIT, [&] { (void)watch::frame::decode(malformed); });
    watch::state invalid(epoch(0x42), 10, 100, 65536);
    refused(MFW_LIMIT, [&] { (void)invalid.receive(malformed); });
    require(invalid.snapshot().phase == MFW_UNKNOWN_PHASE,
            "malformed wire not sticky");
    watch::state oversized(epoch(0x42), 10, 100, 65536);
    std::vector<uint8_t> large(MFW_MAX_WIRE_BYTES + 1u);
    refused(MFW_LIMIT, [&] { (void)oversized.receive(large); });
    require(oversized.snapshot().phase == MFW_UNKNOWN_PHASE,
            "oversized wire not sticky");
    passed("budget-malformed-and-oversized-refusals");
  }
  for (uint32_t end = 1; end <= 5; end++) {
    const auto wire =
        read(directory + "/terminal-" + std::to_string(end) + ".capnp");
    watch::state state(epoch(0x43), 1, 0, 65536);
    auto frame = state.receive(wire);
    require(frame.info().end == end && frame.info().changes == 0,
            "terminal carried partial changes");
    require(frame.encode() == wire, "terminal roundtrip");
    state.acknowledge(1, watch::hash(wire));
    state.transport_lost();
    require(state.snapshot().phase == MFW_ENDED_PHASE &&
                state.snapshot().end == end,
            "terminal history lost");
  }
  passed("five-synthetic-terminal-kinds");
  {
    auto frame = watch::frame::decode(unpaired);
    const auto view = frame.change(0);
    const std::vector<uint8_t> expected = {0, 0xd8, 0x61, 0};
    require(view.name_bytes == expected.size() &&
                std::equal(expected.begin(), expected.end(), view.name),
            "unpaired surrogate normalized");
    require(frame.encode() == unpaired, "unpaired surrogate roundtrip");
    passed("unpaired-utf16-preserved");
  }
  {
    static_assert(sizeof(size_t) > sizeof(uint32_t),
                  "this is a Windows x64 native conformance program");
    const size_t excessive =
        static_cast<size_t>(std::numeric_limits<uint32_t>::max()) + 1;
    for (uint32_t phase = 0; phase < 4; phase++) {
      const auto ep = epoch(phase == 2 ? 0x43 : 0x42);
      mfw_state_limits_v1 limits{1, sizeof(limits), 10, 100, 65536, 0};
      mfw_state_v1 *owner = nullptr;
      mfw_frame_v1 *frame = nullptr;
      watch::checked(
          mfw_state_v1_new(ep.data(), &limits, sizeof(limits), &owner));
      if (phase == 1)
        watch::checked(mfw_state_v1_receive(
            owner, original[0].data(), watch::wire_length(original[0].size()),
            &frame));
      if (phase == 2) {
        watch::checked(mfw_state_v1_receive(
            owner, gap.data(), watch::wire_length(gap.size()), &frame));
        const auto digest = watch::hash(gap);
        watch::checked(mfw_state_v1_ack(owner, 1, digest.data()));
      }
      if (phase == 3)
        watch::checked(mfw_state_v1_transport_lost(owner));
      const uint32_t code =
          phase == 0 ? MFW_LIMIT
                     : (phase == 1 ? MFW_PENDING
                                   : (phase == 2 ? MFW_STATE : MFW_UNKNOWN));
      refused(code,
              [&] { (void)watch::detail::receive_length(owner, excessive); });
      mfw_state_snapshot_v1 value{};
      watch::checked(mfw_state_v1_snapshot(owner, &value, sizeof(value)));
      require(value.phase == (phase == 0 ? MFW_UNKNOWN_PHASE : phase),
              "wide input guard changed phase precedence");
      require(value.frames == ((phase == 1 || phase == 2) ? 1u : 0u),
              "wide input guard changed charge");
      mfw_frame_v1_free(frame);
      mfw_state_v1_free(owner);
    }
    passed("native-size-width-guard-preserves-phase-precedence");
  }
  std::cout
      << "CPP_WATCH_CONFORMANCE_PASS scenarios=" << scenarios
      << " assertions=" << checks
      << " original_native_payloads=6 synthetic_vectors=7 native_authority=0\n";
  return 0;
} catch (const std::exception &error) {
  std::cerr << "CPP_FAILURE " << error.what() << '\n';
  return 2;
}
