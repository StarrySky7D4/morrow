#ifndef MORROW_WS_MESSAGE_V1_H
#define MORROW_WS_MESSAGE_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MWS_V1_OK 0u
#define MWS_V1_INVALID 1u
#define MWS_V1_CONTRACT 2u
#define MWS_V1_LIMIT 3u
#define MWS_V1_UTF8 4u
#define MWS_V1_BUFFER 5u
#define MWS_V1_TEXT 0u
#define MWS_V1_BINARY 1u
#define MWS_V1_PING 2u
#define MWS_V1_PONG 3u
#define MWS_V1_CLOSE 4u
#define MWS_V1_VERSION 1u
#define MWS_V1_MAX_ENVELOPE_BYTES 65536u
#define MWS_V1_MAX_PAYLOAD_BYTES 65536u
#define MWS_V1_MAX_CONTROL_BYTES 125u
#define MWS_V1_MAX_CLOSE_BYTES 123u
/* Codec identity only. This library creates no grant, socket or guest import.
 * Fully owned output; payload has explicit length and no NUL termination.
 * All inputs must remain readable during the call. Nonnull message/out/length
 * pointers must be correctly aligned. Out_size/message_size >= sizeof(struct).
 * Inputs and outputs may alias: all input is consumed before output is written.
 * Encode's output length must not overlap its written byte prefix (checked).
 * EVERY error leaves output bytes and output length unchanged.
 * Success retains no pointers; tail payload storage in set/decode is zeroed.
 * Null payload is permitted in set only when length is zero.
 * Encode capacity is a writable byte count; unused bytes remain unchanged.
 * A payload fitting MAX_PAYLOAD_BYTES may exceed the serialized envelope cap:
 * actual Capnp allocator/segment overhead is included by encode. */
typedef struct mws_message_v1 {
    uint32_t kind, has_close_code, payload_length;
    uint16_t close_code, reserved;
    uint8_t payload[MWS_V1_MAX_PAYLOAD_BYTES];
} mws_message_v1;
uint32_t mws_message_v1_decode(const uint8_t *bytes, uint32_t length, mws_message_v1 *out, uint32_t out_size);
uint32_t mws_message_v1_set(uint32_t kind, const uint8_t *payload, uint32_t length,
    uint32_t has_close_code, uint16_t close_code, mws_message_v1 *out, uint32_t out_size);
uint32_t mws_message_v1_encode(const mws_message_v1 *message, uint32_t message_size,
    uint8_t *out, uint32_t capacity, uint32_t *out_length);
uint32_t mws_message_v1_schema_digest(uint8_t *out32, uint32_t capacity);
#ifdef __cplusplus
}
#endif
#endif
