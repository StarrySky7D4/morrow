#ifndef MORROW_CHANGES_METADATA_V1_H
#define MORROW_CHANGES_METADATA_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MC_METADATA_V1_OK 0u
#define MC_METADATA_V1_INVALID 1u
#define MC_METADATA_V1_CONTRACT 2u
#define MC_METADATA_V1_CORRELATION 3u
#define MC_METADATA_V1_LIMIT 4u
#define MC_METADATA_V1_MAX_BYTES 662u
/* Owned output, no pointers. IDs have explicit lengths; no NUL termination.
 * Card hash covers all Card.encode() bytes. No value grants authority.
 * Inputs must remain readable during the call. Output must be aligned and
 * writable for sizeof(mc_metadata_v1). Error leaves output unchanged. */
typedef struct mc_metadata_v1 {
    uint8_t scope_digest[32], window_id[32], card_sha256[32];
    uint64_t revision;
    uint32_t card_id_length, operation_id_length;
    uint8_t card_id[256], operation_id[256];
} mc_metadata_v1;
uint32_t mc_metadata_v1_decode(const uint8_t *bytes, uint32_t length,
    const uint8_t *epoch32, const uint8_t *cursor, uint32_t cursor_length,
    mc_metadata_v1 *out, uint32_t out_size);
#ifdef __cplusplus
}
#endif
#endif
