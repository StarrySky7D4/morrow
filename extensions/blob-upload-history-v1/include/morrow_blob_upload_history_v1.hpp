#ifndef MORROW_BLOB_UPLOAD_HISTORY_V1_HPP
#define MORROW_BLOB_UPLOAD_HISTORY_V1_HPP
#include "morrow_blob_upload_history_v1.h"
#include <array>
#include <memory>
#include <stdexcept>
#include <string_view>
#include <vector>
namespace morrow::upload_history_v1 {
class Packet {
  struct Delete { void operator()(MbhPacket *p) const noexcept { mbh_packet_free(p); } };
  std::unique_ptr<MbhPacket, Delete> owner_;
public:
  Packet() = default;
  Packet(const Packet &) = delete;
  Packet &operator=(const Packet &) = delete;
  Packet(Packet &&) noexcept = default;
  Packet &operator=(Packet &&) noexcept = default;
  uint32_t decode_task(const std::vector<uint8_t> &bytes, std::string_view task_id) {
    if (bytes.empty() || bytes.size() > MBH_V1_MAX_PACKET_BYTES) return 3;
    if (task_id.size() != MBH_V1_TASK_ID_BYTES) return 4;
    MbhPacket *candidate = nullptr;
    const auto status = mbh_packet_decode_task(bytes.data(), static_cast<uint32_t>(bytes.size()),
        reinterpret_cast<const uint8_t*>(task_id.data()), static_cast<uint32_t>(task_id.size()), &candidate);
    if (!status) owner_.reset(candidate);
    return status;
  }
  uint32_t decode(const std::vector<uint8_t> &bytes,
                  const std::array<uint8_t, 32> &submission,
                  const std::array<uint8_t, 32> &nonce) {
    if (bytes.empty() || bytes.size() > MBH_V1_MAX_PACKET_BYTES) return 3;
    MbhPacket *candidate = nullptr;
    const auto status = mbh_packet_decode(bytes.data(), static_cast<uint32_t>(bytes.size()), submission.data(), nonce.data(), &candidate);
    if (!status) owner_.reset(candidate);
    return status;
  }
  // Views borrow this exact owner; a successful new decode invalidates old views.
  uint32_t view(MbhView &out) const noexcept {
    return mbh_packet_view(owner_.get(), &out, static_cast<uint32_t>(sizeof out));
  }
};
}
#endif
