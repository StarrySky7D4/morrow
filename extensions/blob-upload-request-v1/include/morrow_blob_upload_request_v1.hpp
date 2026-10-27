#ifndef MORROW_BLOB_UPLOAD_REQUEST_V1_HPP
#define MORROW_BLOB_UPLOAD_REQUEST_V1_HPP
#include "morrow_blob_upload_request_v1.h"
#include <utility>
#include <vector>

namespace morrow::blob_upload_request_v1 {
class request {
    mbu_request *handle_ = nullptr;
public:
    request() = default;
    ~request() { mbu_request_free(handle_); }
    request(const request&) = delete;
    request& operator=(const request&) = delete;
    request(request&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    request& operator=(request&& other) noexcept {
        if (this != &other) { mbu_request_free(handle_); handle_ = std::exchange(other.handle_, nullptr); }
        return *this;
    }
    const mbu_request* native() const noexcept { return handle_; }
    void adopt(mbu_request *value) noexcept {
        if (value != handle_) { mbu_request_free(handle_); handle_ = value; }
    }
    uint32_t assign(const mbu_request_view& view) {
        mbu_request *value = nullptr;
        auto code = mbu_request_new(&view, sizeof(view), &value);
        if (code == MBU_OK) adopt(value);
        return code;
    }
    uint32_t decode(const uint8_t *bytes, uint32_t length) {
        mbu_request *value = nullptr;
        auto code = mbu_request_decode(bytes, length, &value);
        if (code == MBU_OK) adopt(value);
        return code;
    }
    uint32_t view(mbu_request_view& value) const { return mbu_request_view_get(handle_, &value, sizeof(value)); }
    uint32_t encode(std::vector<uint8_t>& output) const {
        std::vector<uint8_t> temporary(MBU_MAX_REQUEST_BYTES);
        uint32_t length = 0;
        auto code = mbu_request_encode(handle_, temporary.data(), static_cast<uint32_t>(temporary.size()), &length);
        if (code == MBU_OK) { temporary.resize(length); output.swap(temporary); }
        return code;
    }
};
class response {
    mbu_response *handle_ = nullptr;
public:
    response() = default;
    ~response() { mbu_response_free(handle_); }
    response(const response&) = delete;
    response& operator=(const response&) = delete;
    response(response&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    response& operator=(response&& other) noexcept {
        if (this != &other) { mbu_response_free(handle_); handle_ = std::exchange(other.handle_, nullptr); }
        return *this;
    }
    const mbu_response* native() const noexcept { return handle_; }
    void adopt(mbu_response *value) noexcept {
        if (value != handle_) { mbu_response_free(handle_); handle_ = value; }
    }
    uint32_t assign(const request& original, const mbu_reply_view& view) {
        mbu_response *value = nullptr;
        auto code = mbu_response_new(original.native(), &view, sizeof(view), &value);
        if (code == MBU_OK) adopt(value);
        return code;
    }
    uint32_t decode_for(const uint8_t *bytes, uint32_t length, const request& original) {
        mbu_response *value = nullptr;
        auto code = mbu_response_decode_for(bytes, length, original.native(), &value);
        if (code == MBU_OK) adopt(value);
        return code;
    }
    /* receipt in this view is read-only and expires when this response changes. */
    uint32_t view(mbu_reply_view& value) const { return mbu_response_view_get(handle_, &value, sizeof(value)); }
    uint32_t encode(std::vector<uint8_t>& output) const {
        std::vector<uint8_t> temporary(MBU_MAX_RESPONSE_BYTES);
        uint32_t length = 0;
        auto code = mbu_response_encode(handle_, temporary.data(), static_cast<uint32_t>(temporary.size()), &length);
        if (code == MBU_OK) { temporary.resize(length); output.swap(temporary); }
        return code;
    }
};
class client {
    mbu_client *handle_ = nullptr;
public:
    client() = default;
    ~client() { mbu_client_free(handle_); }
    client(const client&) = delete;
    client& operator=(const client&) = delete;
    client(client&& other) noexcept : handle_(std::exchange(other.handle_, nullptr)) {}
    client& operator=(client&& other) noexcept {
        if (this != &other) { mbu_client_free(handle_); handle_ = std::exchange(other.handle_, nullptr); }
        return *this;
    }
    uint32_t initialize(const mbu_client_options& options) {
        /* No hidden reset of an old client's Unknown, pending request or fees. */
        if (handle_ != nullptr) return MBU_STATE;
        return mbu_client_new(&options, sizeof(options), &handle_);
    }
    uint32_t snapshot(mbu_snapshot& value) const { return mbu_client_snapshot(handle_, &value, sizeof(value)); }
    uint32_t begin(const mbu_request_view& view, request& output) {
        mbu_request *value = nullptr;
        auto code = mbu_client_begin(handle_, &view, sizeof(view), &value);
        if (code == MBU_OK) output.adopt(value);
        return code;
    }
    uint32_t accept(const uint8_t *bytes, uint32_t length, response& output) {
        mbu_response *value = nullptr;
        auto code = mbu_client_accept(handle_, bytes, length, &value);
        if (code == MBU_OK) output.adopt(value);
        return code;
    }
    uint32_t call_once(const mbu_request_view& view, mbu_transport_fn callback, void *context, response& output) {
        mbu_response *value = nullptr;
        auto code = mbu_client_call_once(handle_, &view, sizeof(view), callback, context, &value);
        if (code == MBU_OK) output.adopt(value);
        return code;
    }
    uint32_t transport_unknown() { return mbu_client_transport_unknown(handle_); }
    uint32_t abandon() { return mbu_client_abandon(handle_); }
};
}
#endif
