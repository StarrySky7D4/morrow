#ifndef MORROW_SSE_EVENT_V1_H
#define MORROW_SSE_EVENT_V1_H
#include <stdint.h>
#define MSE_V1_OK 0u
#define MSE_V1_INVALID 1u
#define MSE_V1_CONTRACT 2u
#define MSE_V1_LIMIT 3u
#define MSE_V1_UTF8 4u
#define MSE_V1_BUFFER 5u
#define MSE_V1_VERSION 1u
#define MSE_V1_MAX_ENVELOPE_BYTES 65536u
#define MSE_V1_MAX_FIELD_BYTES 65536u
/* Native callers supply valid readable/writable memory. These functions are
 * codecs, not a pointer sandbox. Lengths/offsets count bytes, not characters.
 * All fields are full UTF-8; no parser id/retry truncation policy is applied.
 * Each offset+length must fit storage, summed logical lengths <=65536, reserved0,
 * has_retry0/1 and retry0 when absent. Valid ranges may overlap. */
typedef struct mse_event_v1 {
    uint32_t data_offset, data_length, event_offset, event_length, id_offset, id_length;
    uint32_t has_retry, reserved;
    uint64_t retry;
    uint8_t storage[MSE_V1_MAX_FIELD_BYTES];
} mse_event_v1;
#ifdef __cplusplus
extern "C" {
#endif
/* Output must be aligned/writable for sizeof(mse_event_v1), out_size >=sizeof.
 * Decode input must be readable for length. Set field pointers must be readable
 * for their lengths; null is permitted only for zero-length fields.
 * Input/output may alias: all inputs consumed before writing. Every error leaves
 * output unchanged. Success packs fields in data,event,id order, zeroes unused
 * storage and retains no input pointer. */
uint32_t mse_event_v1_decode(const uint8_t*,uint32_t,mse_event_v1*,uint32_t);
uint32_t mse_event_v1_set(const uint8_t*,uint32_t,const uint8_t*,uint32_t,const uint8_t*,uint32_t,uint32_t,uint64_t,mse_event_v1*,uint32_t);
/* Event must be aligned/readable for sizeof, event_size >=sizeof. Output must be
 * writable for capacity bytes, out_length aligned/writable for uint32_t. Input
 * may alias outputs; encoded prefix and out_length may not overlap (checked).
 * Every error leaves both outputs unchanged. Success writes only encoded prefix
 * and length. A valid field aggregate or accepted wire can still encode LIMIT
 * because the native default allocator has genuine serialized overhead. */
uint32_t mse_event_v1_encode(const mse_event_v1*,uint32_t,uint8_t*,uint32_t,uint32_t*);
/* Writes exactly32 bytes only on success; insufficient capacity changes nothing. */
uint32_t mse_event_v1_schema_digest(uint8_t*,uint32_t);
#ifdef __cplusplus
}
#endif
#endif
