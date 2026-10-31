#include "morrow_fs_watch_v1_status.hpp"
#include <algorithm>
#include <type_traits>
extern "C" {
__attribute__((import_module("morrow_task_v1"),
               import_name("read_input"))) int32_t
task_read(uint8_t *, uint32_t);
__attribute__((import_module("morrow_task_v1"),
               import_name("complete"))) int32_t
task_complete(const uint8_t *, uint32_t);
}
namespace watch = morrow::fs_watch_v1_status;
static_assert(sizeof(void *) == 4 && sizeof(mfw_change_view_v1) == 20,
              "Wasm32 ABI");
static_assert(!std::is_copy_constructible_v<watch::frame> &&
              !std::is_copy_constructible_v<watch::state>);
static_assert(std::is_nothrow_move_constructible_v<watch::frame> &&
              std::is_nothrow_move_constructible_v<watch::state>);
static bool state_check(const std::vector<uint8_t> &input,
                        const mfw_frame_info_v1 &info) {
  watch::state original;
  watch::frame accepted, second;
  watch::digest epoch{}, digest{};
  std::copy(std::begin(info.epoch), std::end(info.epoch), epoch.begin());
  mfw_state_limits_v1 limits{1, sizeof(limits), 2, 512, 131072, 0};
  mfw_state_snapshot_v1 snapshot{};
  if (original.initialize(epoch, limits) != MFW_OK ||
      original.initialize(epoch, limits) != MFW_STATE)
    return false;
  if (original.receive(input.data(), input.size(), accepted) != MFW_OK ||
      watch::hash(input.data(), input.size(), digest) != MFW_OK)
    return false;
  watch::state state(std::move(original));
  if (original.snapshot(snapshot) != MFW_INVALID)
    return false;
  if (state.snapshot(snapshot) != MFW_OK ||
      snapshot.phase != MFW_PENDING_PHASE || snapshot.frames != 1 ||
      snapshot.wire_bytes != input.size())
    return false;
  if (state.receive(input.data(), input.size(), second) != MFW_PENDING ||
      second.has_value() || state.acknowledge(2, digest) != MFW_CORRELATION)
    return false;
  if (state.acknowledge(1, digest) != MFW_OK ||
      state.transport_lost() != MFW_OK || state.snapshot(snapshot) != MFW_OK)
    return false;
  return snapshot.phase ==
             (info.end == MFW_CONTINUE ? MFW_UNKNOWN_PHASE : MFW_ENDED_PHASE) &&
         snapshot.frames == 1 && snapshot.wire_bytes == input.size() &&
         state.acknowledge(1, digest) ==
             (info.end == MFW_CONTINUE ? MFW_UNKNOWN : MFW_STATE);
}
extern "C" int32_t morrow_run() {
  std::vector<uint8_t> input(131072), encoded;
  int32_t length = task_read(input.data(), static_cast<uint32_t>(input.size()));
  if (length <= 0 || static_cast<uint32_t>(length) > MFW_MAX_WIRE_BYTES)
    return -1;
  input.resize(static_cast<size_t>(length));
  watch::frame frame;
  mfw_frame_info_v1 info{};
  if (frame.decode(input.data(), input.size()) != MFW_OK ||
      frame.info(info) != MFW_OK)
    return -1;
  for (uint32_t i = 0; i < info.changes; i++) {
    mfw_change_view_v1 view{};
    if (frame.change(i, view) != MFW_OK || view.name_bytes == 0 ||
        view.name_bytes % 2 != 0)
      return -1;
  }
  watch::frame moved(std::move(frame));
  mfw_frame_info_v1 invalid{};
  if (frame.has_value() || frame.info(invalid) != MFW_INVALID ||
      moved.encode(encoded) != MFW_OK || encoded != input)
    return -1;
  if (info.sequence == 1 && !state_check(input, info))
    return -1;
  return task_complete(encoded.data(), static_cast<uint32_t>(encoded.size()));
}
