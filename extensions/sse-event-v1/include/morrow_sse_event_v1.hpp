#ifndef MORROW_SSE_EVENT_V1_HPP
#define MORROW_SSE_EVENT_V1_HPP
#include "morrow_sse_event_v1.h"
#include <memory>
#include <optional>
#include <string_view>
#include <vector>
namespace morrow { namespace sse_event_v1 {
// Move-only owner. Views are valid until successful assignment/decode, move or
// destruction. Moved-from methods reject; its view is an empty sentinel.
// C++ allocation follows ordinary library semantics; OOM is not a codec status.
class event {
    std::unique_ptr<mse_event_v1> value_;
public:
    event() : value_(std::make_unique<mse_event_v1>()) {}
    event(event&&) noexcept = default;
    event& operator=(event&&) noexcept = default;
    event(const event&) = delete;
    event& operator=(const event&) = delete;
    const mse_event_v1& view() const noexcept {
        static const mse_event_v1 empty{};
        return value_ ? *value_ : empty;
    }
    uint32_t assign(const uint8_t* data,uint32_t data_length,
                    const uint8_t* type,uint32_t type_length,
                    const uint8_t* id,uint32_t id_length,
                    std::optional<uint64_t> retry=std::nullopt) noexcept {
        if (!value_) return MSE_V1_INVALID;
        return mse_event_v1_set(data,data_length,type,type_length,id,id_length,
            retry ? 1u : 0u,retry ? *retry : 0u,value_.get(),sizeof(mse_event_v1));
    }
    uint32_t assign(std::string_view data,std::string_view type,std::string_view id,
                    std::optional<uint64_t> retry=std::nullopt) noexcept {
        if (!value_) return MSE_V1_INVALID;
        if (data.size()>MSE_V1_MAX_FIELD_BYTES || type.size()>MSE_V1_MAX_FIELD_BYTES ||
            id.size()>MSE_V1_MAX_FIELD_BYTES || data.size()+type.size()+id.size()>MSE_V1_MAX_FIELD_BYTES)
            return MSE_V1_LIMIT;
        return assign(reinterpret_cast<const uint8_t*>(data.data()),static_cast<uint32_t>(data.size()),
                      reinterpret_cast<const uint8_t*>(type.data()),static_cast<uint32_t>(type.size()),
                      reinterpret_cast<const uint8_t*>(id.data()),static_cast<uint32_t>(id.size()),retry);
    }
    uint32_t decode(const uint8_t* bytes,uint32_t length) noexcept {
        if (!value_) return MSE_V1_INVALID;
        return mse_event_v1_decode(bytes,length,value_.get(),sizeof(mse_event_v1));
    }
    // Message and existing vector both remain unchanged on any codec error.
    uint32_t encode(std::vector<uint8_t>& out) const {
        if (!value_) return MSE_V1_INVALID;
        std::vector<uint8_t> encoded(MSE_V1_MAX_ENVELOPE_BYTES);
        uint32_t length=0;
        const auto status=mse_event_v1_encode(value_.get(),sizeof(mse_event_v1),encoded.data(),static_cast<uint32_t>(encoded.size()),&length);
        if (status==MSE_V1_OK) { encoded.resize(length); out.swap(encoded); }
        return status;
    }
};
} }
#endif
