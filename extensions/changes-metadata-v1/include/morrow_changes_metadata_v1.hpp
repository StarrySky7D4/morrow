#ifndef MORROW_CHANGES_METADATA_V1_HPP
#define MORROW_CHANGES_METADATA_V1_HPP
#include "morrow_changes_metadata_v1.h"
#include <array>
namespace morrow { namespace changes_metadata_v1 {
struct metadata {
    mc_metadata_v1 value{};
    uint32_t decode(const uint8_t *bytes, uint32_t length,
                    const uint8_t *epoch32, const uint8_t *cursor, uint32_t cursor_length) noexcept {
        return mc_metadata_v1_decode(bytes,length,epoch32,cursor,cursor_length,&value,sizeof(value));
    }
};
} }
#endif
