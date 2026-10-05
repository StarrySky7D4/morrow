#ifndef MORROW_WS_MESSAGE_V1_HPP
#define MORROW_WS_MESSAGE_V1_HPP
#include "morrow_ws_message_v1.h"
#include <memory>
#include <optional>
#include <vector>
namespace morrow { namespace ws_message_v1 {
enum class kind : uint32_t { Text=MWS_V1_TEXT, Binary=MWS_V1_BINARY, Ping=MWS_V1_PING, Pong=MWS_V1_PONG, Close=MWS_V1_CLOSE };
// Move-only owner: decoded payloads never borrow caller wire buffers. Views remain
// valid until successful reassignment/decode, move or destruction. Moved-from
// methods reject; its view returns an empty sentinel and retains no borrowed data.
// Allocation follows ordinary C++ library semantics; OOM is not a codec status.
class message {
    std::unique_ptr<mws_message_v1> value_;
public:
    message() : value_(std::make_unique<mws_message_v1>()) {}
    message(message&&) noexcept = default;
    message& operator=(message&&) noexcept = default;
    message(const message&) = delete;
    message& operator=(const message&) = delete;
    const mws_message_v1& view() const noexcept {
        static const mws_message_v1 empty{};
        return value_ ? *value_ : empty;
    }
    uint32_t assign(kind type, const uint8_t *payload, uint32_t length,
                    std::optional<uint16_t> close_code=std::nullopt) noexcept {
        if (!value_) return MWS_V1_INVALID;
        return mws_message_v1_set(static_cast<uint32_t>(type),payload,length,
            close_code ? 1u : 0u,close_code ? *close_code : 0u,value_.get(),sizeof(mws_message_v1));
    }
    uint32_t decode(const uint8_t *bytes, uint32_t length) noexcept {
        if (!value_) return MWS_V1_INVALID;
        return mws_message_v1_decode(bytes,length,value_.get(),sizeof(mws_message_v1));
    }
    // Both the existing vector and this message remain unchanged on codec error.
    uint32_t encode(std::vector<uint8_t>& out) const {
        if (!value_) return MWS_V1_INVALID;
        std::vector<uint8_t> encoded(MWS_V1_MAX_ENVELOPE_BYTES);
        uint32_t length=0;
        const auto status=mws_message_v1_encode(value_.get(),sizeof(mws_message_v1),encoded.data(),static_cast<uint32_t>(encoded.size()),&length);
        if (status==MWS_V1_OK) { encoded.resize(length); out.swap(encoded); }
        return status;
    }
};
} }
#endif
