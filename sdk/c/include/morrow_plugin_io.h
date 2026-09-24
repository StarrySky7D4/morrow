#ifndef MORROW_PLUGIN_IO_H
#define MORROW_PLUGIN_IO_H
#include "morrow_plugin_codec.h"
#include "morrow_plugin_sdk.h"
#ifdef __cplusplus
extern "C" {
#endif

#define MP_IO_ABI_VERSION 1u
#define MP_MAX_IO_FRAME_BYTES 131072u
#define MP_MAX_IO_PAYLOAD_BYTES 65536u
#define MP_MAX_IO_HEADERS 64u
/* IO-only codec result for a known schema action outside this SDK profile. */
#define MP_IO_CODEC_UNSUPPORTED 21u
#define MP_IO_FILE_READ 1u
#define MP_IO_HTTP_REQUEST 2u
#define MP_IO_POLL 3u
#define MP_IO_READ 4u
#define MP_IO_FINISH 5u
#define MP_IO_CANCEL 6u
#define MP_IO_QUERY_OPERATION 7u
#define MP_IO_STATUS_INVALID 0u
#define MP_IO_STATUS_ACCEPTED 1u
#define MP_IO_STATUS_PENDING 2u
#define MP_IO_STATUS_COMPLETED 3u
#define MP_IO_STATUS_DENIED 4u
#define MP_IO_STATUS_REVOKED 5u
#define MP_IO_STATUS_EXPIRED 6u
#define MP_IO_STATUS_UNSUPPORTED 7u
#define MP_IO_STATUS_QUOTA 8u
#define MP_IO_STATUS_NOT_FOUND 9u
#define MP_IO_STATUS_CONFLICT 10u
#define MP_IO_STATUS_CANCELLED 11u
#define MP_IO_STATUS_OUTCOME_UNKNOWN 12u
#define MP_IO_STATUS_EVIDENCE_UNAVAILABLE 13u
#define MP_IO_STATUS_FAILED 14u

typedef struct mp_io_header { mp_span name, value; } mp_io_header;
/* All spans are borrowed during encode. A reference is exactly 32 opaque bytes.
 * Endpoint/credential and operation ID are opaque references, never URL, path,
 * secret, authority or permission. The host still applies its own admission.
 * Pointer arrays, descriptor, output and length must be aligned and disjoint. */
typedef struct mp_io_request_v1 {
  uint32_t abi_version, struct_size, kind;
  uint64_t call_id;
  mp_span reference, operation_id, endpoint, method, relative_target, credential, body;
  const mp_io_header *headers;
  uint32_t header_count;
  uint64_t deadline_ms, offset;
  uint32_t limit;
} mp_io_request_v1;
typedef struct mp_io_response mp_io_response;
/* Views borrow an SDK-owned response; all pointers expire at free.
 * encoded_frame is the exact validated host response for broker completion. */
typedef struct mp_io_response_view {
  uint32_t status;
  mp_span reference, bytes, encoded_frame;
  uint64_t offset;
  uint32_t eof, http_status;
  const mp_io_header *headers;
  uint32_t header_count;
} mp_io_response_view;
/* Pure codec only. On failure, output length is zero and buffer is untouched. */
uint32_t mp_io_request_encode(const mp_io_request_v1*,uint8_t*,uint32_t,uint32_t*);
/* Rejects malformed or unsupported IO actions before any host import. */
uint32_t mp_io_request_validate(const uint8_t*,uint32_t);
uint32_t mp_io_schema_digest(uint8_t*,uint32_t);
/* Verifies the response against the exact immutable request frame sent, including
 * its byte layout and hash. out is NULL on failure. No automatic retry. */
uint32_t mp_io_response_decode(const uint8_t*,uint32_t,const uint8_t *request_frame,uint32_t request_length,mp_io_response**);
uint32_t mp_io_response_get(const mp_io_response*,mp_io_response_view*,uint32_t);
void mp_io_response_free(mp_io_response*);
#ifdef __cplusplus
}
#endif

#if defined(__wasm32__)
#include <stdlib.h>
#include <string.h>
#ifdef __cplusplus
extern "C" {
#endif
__attribute__((import_module("morrow_io_v1"), import_name("call")))
int32_t mp_io_import_call(const uint8_t*,uint32_t,uint8_t*,uint32_t);
/* This inline transport creates an import only when called. One host call;
 * transport failure does not prove rollback and is never retried. */
static inline uint32_t mp_wasm_io_call_frame(const uint8_t *request_frame,uint32_t request_length,mp_io_response **out) {
  uint8_t *input, *response; uint32_t status; int32_t written;
  if (!out) return MP_CODEC_INVALID;
  *out=NULL;
  if (!request_frame || request_length==0) return MP_CODEC_INVALID;
  if (request_length>MP_MAX_IO_FRAME_BYTES) return MP_CODEC_LIMIT;
  input=(uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  response=(uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  if (!input || !response) { free(input); free(response); return MP_NO_MEMORY; }
  memcpy(input,request_frame,request_length);
  status=mp_io_request_validate(input,request_length);
  if (status!=MP_CODEC_OK) { free(input); free(response); return status; }
  written=mp_io_import_call(input,request_length,response,MP_MAX_IO_FRAME_BYTES);
  if (written<=0 || (uint32_t)written>MP_MAX_IO_FRAME_BYTES) status=MP_TRANSPORT_FAILURE;
  else status=mp_io_response_decode(response,(uint32_t)written,input,request_length,out);
  free(input); free(response); return status;
}
static inline uint32_t mp_wasm_io_call(const mp_io_request_v1 *request,mp_io_response **out) {
  uint8_t *input; uint32_t length=0, status;
  if (!out) return MP_CODEC_INVALID;
  *out=NULL;
  input=(uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  if (!input) return MP_NO_MEMORY;
  status=mp_io_request_encode(request,input,MP_MAX_IO_FRAME_BYTES,&length);
  if (status==MP_CODEC_OK) status=mp_wasm_io_call_frame(input,length,out);
  free(input); return status;
}
#ifdef __cplusplus
}
#endif
#endif
#endif
