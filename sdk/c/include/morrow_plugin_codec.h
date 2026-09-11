#ifndef MORROW_PLUGIN_CODEC_H
#define MORROW_PLUGIN_CODEC_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MP_CODEC_OK 0u
#define MP_CODEC_INVALID 16u
#define MP_CODEC_LIMIT 17u
#define MP_CODEC_CONTRACT 18u
#define MP_CODEC_CORRELATION 19u
#define MP_CODEC_PANIC 20u
#define MP_REQUEST_RENAME 1u
#define MP_REQUEST_SUMMARY 2u
#define MP_REQUEST_QUERY 3u
#define MP_REQUEST_ATTACHMENT 4u
/* UTF-8 text or raw data with explicit byte length; never a NUL-terminated
 * string. */
typedef struct mp_span {
  const uint8_t *data;
  uint32_t length;
} mp_span;
/* Zero-initialize, then set ABI=1, size, kind, request_id/card_id and action
 * fields. Pointers must remain valid during calls; no pointer is serialized
 * onto the wire. revision and offset are exact unsigned 64-bit values in every
 * SDK.
 */
typedef struct mp_request_v1 {
  uint32_t abi_version, struct_size, kind;
  mp_span request_id, card_id, title, operation_id, attachment_id;
  uint64_t revision, offset;
  uint32_t length;
} mp_request_v1;
typedef struct mp_reply mp_reply;
#define MP_REPLY_RENAMED 1u
#define MP_REPLY_SUMMARY 2u
#define MP_REPLY_REJECTED 3u
#define MP_REPLY_QUERY 4u
#define MP_REPLY_ATTACHMENT 5u
/* Spans are read-only views owned by mp_reply; invalid after mp_reply_free.
 * QUERY state 0 = absent snapshot, 1 = locally committed. Never infer no
 * in-flight work. REJECTED failure is the fixed Cap'n Proto Failure enum
 * (denied=0). Attachment bytes are a part; verify whole length/SHA-256 before
 * publication.
 */
typedef struct mp_reply_view {
  uint32_t kind, failure, state, format_version;
  uint64_t revision, offset, total_length;
  mp_span request_id, card_id, operation_id, event_id, type_id, title, preview,
      attachment_id, sha256, bytes;
} mp_reply_view;
/* All buffer/control objects must be disjoint, properly aligned and
 * caller-owned. No host call or persistence occurs in these pure codec
 * operations.
 */
uint32_t mp_request_encode(const mp_request_v1 *request, uint8_t *output,
                           uint32_t capacity, uint32_t *length);
/* Correlates request ID, reply kind, target, revision/offset and queried
 * operation. out_reply is NULL on every failure. Free successful handles
 * exactly once.
 */
uint32_t mp_reply_decode(const uint8_t *bytes, uint32_t length,
                         const mp_request_v1 *request, mp_reply **out_reply);
uint32_t mp_reply_get(const mp_reply *reply, mp_reply_view *view,
                      uint32_t view_size);
void mp_reply_free(mp_reply *reply);
#ifdef __cplusplus
}
#endif
#endif
