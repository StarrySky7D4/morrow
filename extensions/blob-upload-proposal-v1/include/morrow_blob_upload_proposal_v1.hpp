#ifndef MORROW_BLOB_UPLOAD_PROPOSAL_V1_HPP
#define MORROW_BLOB_UPLOAD_PROPOSAL_V1_HPP
#include "morrow_blob_upload_proposal_v1.h"
#include <utility>
#include <vector>
namespace morrow::blob_upload_proposal_v1 {
class proposal {
    mbp_proposal *handle_ = nullptr;
    void replace(mbp_proposal *value) noexcept { mbp_proposal_free(handle_); handle_ = value; }
public:
    proposal() = default;
    ~proposal() { mbp_proposal_free(handle_); }
    proposal(const proposal&) = delete;
    proposal& operator=(const proposal&) = delete;
    proposal(proposal&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    proposal& operator=(proposal&& other) noexcept {
        if (this != &other) replace(std::exchange(other.handle_, nullptr));
        return *this;
    }
    const mbp_proposal* native() const noexcept { return handle_; }
    uint32_t assign(const mbp_proposal_view& view) {
        mbp_proposal *value = nullptr;
        auto code = mbp_proposal_new(&view, sizeof(view), &value);
        if (code == MBP_OK) replace(value);
        return code;
    }
    uint32_t decode(const uint8_t *bytes, uint32_t length) {
        mbp_proposal *value = nullptr;
        auto code = mbp_proposal_decode(bytes, length, &value);
        if (code == MBP_OK) replace(value);
        return code;
    }
    uint32_t view(mbp_proposal_view& out) const { return mbp_proposal_view_get(handle_, &out, sizeof(out)); }
    uint32_t encode(std::vector<uint8_t>& output) const {
        std::vector<uint8_t> temporary(MBP_MAX_PROPOSAL_BYTES); uint32_t length = 0;
        auto code = mbp_proposal_encode(handle_, temporary.data(), static_cast<uint32_t>(temporary.size()), &length);
        if (code == MBP_OK) { temporary.resize(length); output.swap(temporary); }
        return code;
    }
};
class bound_input {
    mbp_bound_input *handle_ = nullptr;
    void replace(mbp_bound_input *value) noexcept { mbp_bound_free(handle_); handle_ = value; }
public:
    bound_input() = default;
    ~bound_input() { mbp_bound_free(handle_); }
    bound_input(const bound_input&) = delete;
    bound_input& operator=(const bound_input&) = delete;
    bound_input(bound_input&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    bound_input& operator=(bound_input&& other) noexcept {
        if (this != &other) replace(std::exchange(other.handle_, nullptr));
        return *this;
    }
    const mbp_bound_input* native() const noexcept { return handle_; }
    uint32_t bind(const proposal& original, const mbp_identity& identity) {
        mbp_bound_input *value = nullptr;
        auto code = mbp_bound_new(original.native(), &identity, sizeof(identity), &value);
        if (code == MBP_OK) replace(value);
        return code;
    }
    uint32_t decode(const uint8_t *bytes, uint32_t length) {
        mbp_bound_input *value = nullptr;
        auto code = mbp_bound_decode(bytes, length, &value);
        if (code == MBP_OK) replace(value);
        return code;
    }
    uint32_t view(mbp_proposal_view& out) const { return mbp_bound_proposal_view(handle_, &out, sizeof(out)); }
    uint32_t identity(mbp_identity& out) const { return mbp_bound_identity(handle_, &out, sizeof(out)); }
    uint32_t encode(std::vector<uint8_t>& output) const {
        std::vector<uint8_t> temporary(MBP_MAX_BOUND_INPUT_BYTES); uint32_t length = 0;
        auto code = mbp_bound_encode(handle_, temporary.data(), static_cast<uint32_t>(temporary.size()), &length);
        if (code == MBP_OK) { temporary.resize(length); output.swap(temporary); }
        return code;
    }
};
}
#endif
