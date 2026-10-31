#ifndef MORROW_FS_WATCH_V1_HPP
#define MORROW_FS_WATCH_V1_HPP
#include "morrow_fs_watch_v1.h"
#include <array>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
namespace morrow {
namespace fs_watch_v1 {
class error : public std::runtime_error {
  uint32_t code_;

public:
  explicit error(uint32_t code)
      : std::runtime_error("directory observation status " +
                           std::to_string(code)),
        code_(code) {}
  uint32_t code() const noexcept { return code_; }
};
inline void checked(uint32_t code) {
  if (code != MFW_OK)
    throw error(code);
}
namespace detail {
// Preserve the core phase precedence even when a native size_t cannot fit the
// wire ABI. No input is accessed on this branch and prior charges stay intact.
inline uint32_t receive_length(mfw_state_v1 *owner, size_t length) {
  if (length <= std::numeric_limits<uint32_t>::max())
    return static_cast<uint32_t>(length);
  mfw_state_snapshot_v1 value{};
  checked(mfw_state_v1_snapshot(owner, &value, sizeof(value)));
  switch (value.phase) {
  case MFW_PENDING_PHASE:
    throw error(MFW_PENDING);
  case MFW_ENDED_PHASE:
    throw error(MFW_STATE);
  case MFW_UNKNOWN_PHASE:
    throw error(MFW_UNKNOWN);
  case MFW_READY_PHASE:
    checked(mfw_state_v1_transport_lost(owner));
    throw error(MFW_LIMIT);
  default:
    throw error(MFW_STATE);
  }
}
} // namespace detail
inline uint32_t wire_length(size_t size) {
  if (size > std::numeric_limits<uint32_t>::max())
    throw error(MFW_LIMIT);
  return static_cast<uint32_t>(size);
}
using digest = std::array<uint8_t, 32>;
inline digest hash(const std::vector<uint8_t> &wire) {
  digest value{};
  checked(mfw_wire_v1_digest(wire.data(), wire_length(wire.size()),
                             value.data(), 32));
  return value;
}
class state;
class frame {
  mfw_frame_v1 *owner_ = nullptr;
  explicit frame(mfw_frame_v1 *owner) noexcept : owner_(owner) {}
  friend class state;

public:
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
  static frame decode(const std::vector<uint8_t> &wire) {
    mfw_frame_v1 *out = nullptr;
    checked(mfw_frame_v1_decode(wire.data(), wire_length(wire.size()), &out));
    return frame(out);
  }
  mfw_frame_info_v1 info() const {
    mfw_frame_info_v1 out{};
    checked(mfw_frame_v1_info(owner_, &out, sizeof(out)));
    return out;
  }
  // Borrowed raw UTF16LE bytes. Keep this frame alive and never mutate the
  // view.
  mfw_change_view_v1 change(uint32_t index) const {
    mfw_change_view_v1 out{};
    checked(mfw_frame_v1_change(owner_, index, &out, sizeof(out)));
    return out;
  }
  std::vector<uint8_t> encode() const {
    std::vector<uint8_t> out(MFW_MAX_WIRE_BYTES);
    uint32_t n = 0;
    checked(mfw_frame_v1_encode(owner_, out.data(),
                                static_cast<uint32_t>(out.size()), &n));
    out.resize(n);
    return out;
  }
};
class state {
  mfw_state_v1 *owner_ = nullptr;

public:
  state(const digest &epoch, uint32_t frames, uint32_t changes,
        uint64_t bytes) {
    mfw_state_limits_v1 limits{MFW_ABI_VERSION, sizeof(mfw_state_limits_v1),
                               frames,          changes,
                               bytes,           0};
    checked(mfw_state_v1_new(epoch.data(), &limits, sizeof(limits), &owner_));
  }
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
  frame receive(const std::vector<uint8_t> &wire) {
    mfw_frame_v1 *out = nullptr;
    const auto length = detail::receive_length(owner_, wire.size());
    checked(mfw_state_v1_receive(owner_, wire.data(), length, &out));
    return frame(out);
  }
  void acknowledge(uint64_t sequence, const digest &exact_wire_digest) {
    checked(mfw_state_v1_ack(owner_, sequence, exact_wire_digest.data()));
  }
  mfw_state_snapshot_v1 snapshot() const {
    mfw_state_snapshot_v1 out{};
    checked(mfw_state_v1_snapshot(owner_, &out, sizeof(out)));
    return out;
  }
  void transport_lost() { checked(mfw_state_v1_transport_lost(owner_)); }
};
} // namespace fs_watch_v1
} // namespace morrow
#endif
