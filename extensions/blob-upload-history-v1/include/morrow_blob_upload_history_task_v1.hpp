#ifndef MORROW_BLOB_UPLOAD_HISTORY_TASK_V1_HPP
#define MORROW_BLOB_UPLOAD_HISTORY_TASK_V1_HPP
#include "morrow_blob_upload_history_task_v1.h"
#include <memory>
#include <string_view>
#include <vector>
namespace morrow::upload_history_v1 {
class TaskPacket {
  struct Delete { void operator()(MbhPacket *p) const noexcept { mbh_packet_free(p); } };
  std::unique_ptr<MbhPacket, Delete> owner_;
public:
  TaskPacket() = default;
  TaskPacket(const TaskPacket &) = delete;
  TaskPacket &operator=(const TaskPacket &) = delete;
  TaskPacket(TaskPacket &&) noexcept = default;
  TaskPacket &operator=(TaskPacket &&) noexcept = default;
  uint32_t decode(const std::vector<uint8_t> &invocation, std::string_view handler,
                  std::string_view output_type) {
    if (invocation.empty() || invocation.size() > MBH_V1_MAX_TASK_BYTES ||
        handler.size() > 256 || output_type.size() > 256) return 3;
    if (handler.empty() || output_type.empty()) return 1;
    MbhPacket *candidate = nullptr;
    const auto status = mbh_packet_decode_invocation(invocation.data(), static_cast<uint32_t>(invocation.size()),
        reinterpret_cast<const uint8_t*>(handler.data()), static_cast<uint32_t>(handler.size()),
        reinterpret_cast<const uint8_t*>(output_type.data()), static_cast<uint32_t>(output_type.size()), &candidate);
    if (!status) owner_.reset(candidate);
    return status;
  }
  uint32_t view(MbhView &out) const noexcept {
    return mbh_packet_view(owner_.get(), &out, static_cast<uint32_t>(sizeof out));
  }
};
}
#endif
