#ifndef MORROW_FS_WATCH_V1_STATUS_HPP
#define MORROW_FS_WATCH_V1_STATUS_HPP
#include "morrow_fs_watch_v1.h"
#include <array>
#include <cstddef>
#include <utility>
#include <vector>
// Status-returning C++17 owners for builds with exceptions disabled. Allocation
// failure follows the selected runtime; this does not promise OOM recovery.
// Local disposal or reinitialization never retires/refunds an original host.
namespace morrow {
namespace fs_watch_v1_status {
using digest = std::array<uint8_t, 32>;
class state;
class frame {
  mfw_frame_v1 *owner_ = nullptr;
  friend class state;

public:
  frame() noexcept = default;
  ~frame() { mfw_frame_v1_free(owner_); }
  frame(const frame &) = delete;
  frame &operator=(const frame &) = delete;
  frame(frame &&other) noexcept
      : owner_(std::exchange(other.owner_, nullptr)) {}
  frame &operator=(frame &&other) noexcept {
    if (this != &other) {
      mfw_frame_v1_free(owner_);
      owner_ = std::exchange(other.owner_, nullptr);
    }
    return *this;
  }
  bool has_value() const noexcept { return owner_ != nullptr; }
  uint32_t decode(const uint8_t *wire, std::size_t length) {
    if (length > MFW_MAX_WIRE_BYTES)
      return MFW_LIMIT;
    mfw_frame_v1 *next = nullptr;
    const auto code =
        mfw_frame_v1_decode(wire, static_cast<uint32_t>(length), &next);
    if (code == MFW_OK) {
      mfw_frame_v1_free(owner_);
      owner_ = next;
    }
    return code;
  }
  uint32_t info(mfw_frame_info_v1 &output) const {
    return mfw_frame_v1_info(owner_, &output, sizeof(output));
  }
  // Borrowed, immutable raw UTF16LE; valid only while this exact owner
  // survives.
  uint32_t change(uint32_t index, mfw_change_view_v1 &output) const {
    return mfw_frame_v1_change(owner_, index, &output, sizeof(output));
  }
  uint32_t encode(std::vector<uint8_t> &output) const {
    std::vector<uint8_t> next(MFW_MAX_WIRE_BYTES);
    uint32_t written = 0;
    const auto code = mfw_frame_v1_encode(
        owner_, next.data(), static_cast<uint32_t>(next.size()), &written);
    if (code == MFW_OK) {
      next.resize(written);
      output.swap(next);
    }
    return code;
  }
};
class state {
  mfw_state_v1 *owner_ = nullptr;

public:
  state() noexcept = default;
  ~state() { mfw_state_v1_free(owner_); }
  state(const state &) = delete;
  state &operator=(const state &) = delete;
  state(state &&other) noexcept
      : owner_(std::exchange(other.owner_, nullptr)) {}
  state &operator=(state &&other) noexcept {
    if (this != &other) {
      mfw_state_v1_free(owner_);
      owner_ = std::exchange(other.owner_, nullptr);
    }
    return *this;
  }
  uint32_t initialize(const digest &epoch, const mfw_state_limits_v1 &limits) {
    if (owner_)
      return MFW_STATE;
    return mfw_state_v1_new(epoch.data(), &limits, sizeof(limits), &owner_);
  }
  uint32_t snapshot(mfw_state_snapshot_v1 &output) const {
    return mfw_state_v1_snapshot(owner_, &output, sizeof(output));
  }
  uint32_t transport_lost() { return mfw_state_v1_transport_lost(owner_); }
  uint32_t acknowledge(uint64_t sequence, const digest &exact_original_wire) {
    return mfw_state_v1_ack(owner_, sequence, exact_original_wire.data());
  }
  uint32_t receive(const uint8_t *wire, std::size_t length, frame &output) {
    if (length > MFW_MAX_WIRE_BYTES) {
      mfw_state_snapshot_v1 current{};
      const auto code = snapshot(current);
      if (code != MFW_OK)
        return code;
      switch (current.phase) {
      case MFW_PENDING_PHASE:
        return MFW_PENDING;
      case MFW_ENDED_PHASE:
        return MFW_STATE;
      case MFW_UNKNOWN_PHASE:
        return MFW_UNKNOWN;
      case MFW_READY_PHASE: {
        const auto lost = transport_lost();
        return lost == MFW_OK ? MFW_LIMIT : lost;
      }
      default:
        return MFW_STATE;
      }
    }
    mfw_frame_v1 *next = nullptr;
    const auto code = mfw_state_v1_receive(
        owner_, wire, static_cast<uint32_t>(length), &next);
    if (code == MFW_OK) {
      mfw_frame_v1_free(output.owner_);
      output.owner_ = next;
    }
    return code;
  }
};
inline uint32_t hash(const uint8_t *wire, std::size_t length, digest &output) {
  if (length > MFW_MAX_WIRE_BYTES)
    return MFW_LIMIT;
  return mfw_wire_v1_digest(wire, static_cast<uint32_t>(length), output.data(),
                            static_cast<uint32_t>(output.size()));
}
} // namespace fs_watch_v1_status
} // namespace morrow
#endif
