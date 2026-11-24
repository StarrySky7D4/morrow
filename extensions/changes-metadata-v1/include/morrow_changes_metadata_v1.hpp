#ifndef MORROW_CHANGES_METADATA_V1_HPP
#define MORROW_CHANGES_METADATA_V1_HPP
#include "morrow_changes_metadata_v1.h"
#include <array>
namespace morrow { namespace changes_metadata_v1 {
struct bookmark {
    using wire_type = std::array<uint8_t, MC_METADATA_V1_BOOKMARK_BYTES>;
    mc_metadata_v1_bookmark value{};

    uint32_t decode(const uint8_t *wire, uint32_t length) noexcept {
        return mc_metadata_v1_bookmark_decode(wire, length, &value, sizeof(value));
    }
    uint32_t encode(wire_type &wire) const noexcept {
        return mc_metadata_v1_bookmark_encode(&value, wire.data(), wire.size());
    }
};

struct metadata {
    mc_metadata_v1 value{};
    uint32_t decode(const uint8_t *bytes, uint32_t length,
                    const uint8_t *epoch32, const uint8_t *cursor, uint32_t cursor_length) noexcept {
        return mc_metadata_v1_decode(bytes,length,epoch32,cursor,cursor_length,&value,sizeof(value));
    }
};
} }
#endif
