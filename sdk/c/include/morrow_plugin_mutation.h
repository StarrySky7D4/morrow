#ifndef MORROW_PLUGIN_MUTATION_H
#define MORROW_PLUGIN_MUTATION_H
#include "morrow_plugin_codec.h"
#include "morrow_plugin_sdk.h"
#ifdef __cplusplus
extern "C" {
#endif

/* Independent, codec-only mutation profile. Valid frames confer no authority. */
#define MP_MUTATION_ABI_VERSION 1u
#define MP_MAX_MUTATION_FRAME_BYTES 131072u
#define MP_MAX_MUTATION_CHUNK_BYTES 61440u
#define MP_MAX_MUTATION_CONTENT_BYTES 16777216u
#define MP_MAX_MUTATION_OPERATION_BYTES 256u
#define MP_MUTATION_MAX_DEADLINE_MS 30000u
#define MP_MUTATION_CODEC_UNSUPPORTED 21u

#define MP_MUTATION_PREPARE_CREATE 1u
#define MP_MUTATION_PREPARE_DELETE 2u
#define MP_MUTATION_CHUNK 3u
#define MP_MUTATION_COMMIT 4u
#define MP_MUTATION_EXECUTE 5u
#define MP_MUTATION_QUERY 6u
#define MP_MUTATION_CANCEL_PLAN 7u
#define MP_MUTATION_RELEASE 8u

#define MP_MUTATION_STATUS_INVALID 0u
#define MP_MUTATION_STATUS_COMPLETED 1u
#define MP_MUTATION_STATUS_DENIED 2u
#define MP_MUTATION_STATUS_REVOKED 3u
#define MP_MUTATION_STATUS_EXPIRED 4u
#define MP_MUTATION_STATUS_UNSUPPORTED 5u
#define MP_MUTATION_STATUS_QUOTA 6u
#define MP_MUTATION_STATUS_NOT_FOUND 7u
#define MP_MUTATION_STATUS_CONFLICT 8u
#define MP_MUTATION_STATUS_CANCELLED 9u
#define MP_MUTATION_STATUS_OUTCOME_UNKNOWN 10u
#define MP_MUTATION_STATUS_FAILED 11u

#define MP_MUTATION_PHASE_NONE 0u
#define MP_MUTATION_PHASE_ABSENT 1u
#define MP_MUTATION_PHASE_PREPARED 2u
#define MP_MUTATION_PHASE_OUTCOME_UNKNOWN 3u
#define MP_MUTATION_PHASE_OBSERVED 4u
#define MP_MUTATION_PHASE_CANCELLED_BEFORE_DISPATCH 5u

#define MP_MUTATION_EFFECT_UNSPECIFIED 0u
#define MP_MUTATION_EFFECT_OS_SUCCEEDED 1u
#define MP_MUTATION_EFFECT_OS_REJECTED 2u

/* Zero-initialize the descriptor. ABI/version/size and every field are checked.
 * reference and submission are exactly 32 opaque bytes, not all zero. operation_id
 * is 1..256 bytes of UTF-8 without controls, path separators or colon. The
 * host alone binds a selection lease to current rights, target and approval.
 * No path, target, approval, scope or content authority is accepted here.
 * Every unused span must be {NULL,0}; every unused scalar must be zero:
 * only PREPARE_CREATE uses content_length/content_sha256; only CHUNK uses
 * offset/bytes. All descriptors, output slots and spans must be valid,
 * aligned where required, disjoint caller-owned memory during each call.
 * This ABI cannot make an arbitrary invalid address safe to dereference. */
typedef struct mp_mutation_request_v1 {
  uint32_t abi_version, struct_size, kind;
  uint64_t call_id;
  mp_span reference, submission, operation_id;
  uint32_t deadline_ms;
  uint64_t content_length;
  mp_span content_sha256;
  uint64_t offset;
  mp_span bytes;
} mp_mutation_request_v1;

typedef struct mp_mutation_response mp_mutation_response;
/* Views borrow SDK-owned storage. All spans, including the exact encoded
 * response frame, remain stable until mp_mutation_response_free is called. */
typedef struct mp_mutation_response_view {
  uint32_t kind, status, phase, effect;
  uint64_t call_id, staged_bytes;
  uint32_t durable_content;
  mp_span reference, submission, operation_id, encoded_frame;
} mp_mutation_response_view;

/* Pure codec. On failure, *length is zero and output bytes are untouched. */
uint32_t mp_mutation_request_encode(const mp_mutation_request_v1*, uint8_t*, uint32_t, uint32_t*);
uint32_t mp_mutation_request_validate(const uint8_t*, uint32_t);
uint32_t mp_mutation_schema_digest(uint8_t*, uint32_t);
/* Decode the immutable original request frame and correlate reply fields.
 * Failure sets *out to NULL. Transport uncertainty must not be retried here. */
uint32_t mp_mutation_response_decode(const uint8_t*, uint32_t,
                                     const uint8_t *request_frame, uint32_t request_length,
                                     mp_mutation_response** out);
uint32_t mp_mutation_response_get(const mp_mutation_response*, mp_mutation_response_view*, uint32_t);
void mp_mutation_response_free(mp_mutation_response*);

#ifdef __cplusplus
}
#endif

#if defined(__wasm32__)
#include <stdlib.h>
#include <string.h>
#ifdef __cplusplus
extern "C" {
#endif
__attribute__((import_module("morrow_mutation_v1"), import_name("call")))
int32_t mp_mutation_import_call(const uint8_t*, uint32_t, uint8_t*, uint32_t);

/* One synchronous host call. Failure after dispatch is transport-unknown and
 * must not be retried automatically. The original correlation frame stays
 * separate from memory passed to the import. */
static inline uint32_t mp_wasm_mutation_call_frame(
    const uint8_t *request_frame, uint32_t request_length,
    mp_mutation_response **out) {
  uint8_t *original = NULL, *input = NULL, *reply = NULL;
  uint32_t status;
  int32_t written;
  if (!out) return MP_CODEC_INVALID;
  *out = NULL;
  if (!request_frame || request_length == 0) return MP_CODEC_INVALID;
  if (request_length > MP_MAX_MUTATION_FRAME_BYTES) return MP_CODEC_LIMIT;
  original = (uint8_t*)malloc(request_length);
  input = (uint8_t*)malloc(request_length);
  reply = (uint8_t*)malloc(MP_MAX_MUTATION_FRAME_BYTES);
  if (!original || !input || !reply) {
    status = MP_NO_MEMORY;
    goto done;
  }
  memcpy(original, request_frame, request_length);
  status = mp_mutation_request_validate(original, request_length);
  if (status != MP_CODEC_OK) goto done;
  memcpy(input, original, request_length);
  written = mp_mutation_import_call(input, request_length, reply,
                                    MP_MAX_MUTATION_FRAME_BYTES);
  if (memcmp(input, original, request_length) != 0 || written <= 0 ||
      (uint32_t)written > MP_MAX_MUTATION_FRAME_BYTES) {
    status = MP_TRANSPORT_FAILURE;
  } else {
    status = mp_mutation_response_decode(reply, (uint32_t)written, original,
                                         request_length, out);
  }
done:
  free(reply);
  free(input);
  free(original);
  return status;
}

static inline uint32_t mp_wasm_mutation_call(
    const mp_mutation_request_v1 *request, mp_mutation_response **out) {
  uint8_t *frame;
  uint32_t length = 0, status;
  if (!out) return MP_CODEC_INVALID;
  *out = NULL;
  frame = (uint8_t*)malloc(MP_MAX_MUTATION_FRAME_BYTES);
  if (!frame) return MP_NO_MEMORY;
  status = mp_mutation_request_encode(request, frame,
                                      MP_MAX_MUTATION_FRAME_BYTES, &length);
  if (status == MP_CODEC_OK)
    status = mp_wasm_mutation_call_frame(frame, length, out);
  free(frame);
  return status;
}
#ifdef __cplusplus
}
#endif
#endif
#endif
