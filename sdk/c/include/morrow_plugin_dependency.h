#ifndef MORROW_PLUGIN_DEPENDENCY_H
#define MORROW_PLUGIN_DEPENDENCY_H
#include "morrow_plugin_codec.h"
#include "morrow_plugin_sdk.h"
#ifdef __cplusplus
extern "C" {
#endif
#define MP_MAX_DEPENDENCY_BYTES 131072u
#define MP_MAX_DEPENDENCY_VALUE_BYTES 65536u
/* Local codec descriptor: ABI=1, struct_size=sizeof(mp_dependency_request_v1).
 * UTF-8 call_id/slot <=256 bytes, nonempty input <=65536. No provider/host identity.
 * Every pointer/control object must be aligned, disjoint and valid for its length. */
typedef struct mp_dependency_request_v1 {
  uint32_t abi_version, struct_size;
  mp_span call_id, slot, input;
} mp_dependency_request_v1;
typedef struct mp_dependency_output mp_dependency_output;
/* Spans borrow the owned output until free; empty output bytes are valid. */
typedef struct mp_dependency_output_view { mp_span output_type, bytes; } mp_dependency_output_view;
/* Pure codec; no runtime permission, transport, persistence or commit. Encoding leaves
 * output bytes untouched on failure and sets length=0. Digest writes exactly 32 bytes. */
uint32_t mp_dependency_request_encode(const mp_dependency_request_v1*,uint8_t*,uint32_t,uint32_t*);
uint32_t mp_dependency_schema_digest(uint8_t*,uint32_t);
/* Verifies against the exact immutable request frame actually sent (including layout),
 * never reconstructed descriptor fields. Keep that frame unchanged until verification.
 * out=NULL on failure; free exactly once. */
uint32_t mp_dependency_response_decode(const uint8_t*,uint32_t,const uint8_t *request_frame,uint32_t request_length,mp_dependency_output**);
uint32_t mp_dependency_output_get(const mp_dependency_output*,mp_dependency_output_view*,uint32_t);
void mp_dependency_output_free(mp_dependency_output*);
#ifdef __cplusplus
}
#endif

#if defined(__wasm32__)
#include <stdlib.h>
#ifdef __cplusplus
extern "C" {
#endif
__attribute__((import_module("morrow_dependency_v1"), import_name("call")))
int32_t mp_dependency_import_call(const uint8_t*,uint32_t,uint8_t*,uint32_t);
/* Use only after read_input and before complete in the dependency-enabled task mode.
 * This explicit inline transport adds no import to guests that never call it.
 * Allocates disjoint 128KiB buffers before calling, verifies the response, and never retries.
 * A failed call does not imply rollback. Caller retains original descriptor spans throughout. */
static inline uint32_t mp_wasm_dependency_call(const mp_dependency_request_v1 *request,
                                              mp_dependency_output **out) {
  uint8_t *input, *response; uint32_t length=0, status; int32_t written;
  if (!out) return MP_CODEC_INVALID;
  *out=NULL;
  input=(uint8_t*)malloc(MP_MAX_DEPENDENCY_BYTES);
  response=(uint8_t*)malloc(MP_MAX_DEPENDENCY_BYTES);
  if (!input || !response) { free(input); free(response); return MP_NO_MEMORY; }
  status=mp_dependency_request_encode(request,input,MP_MAX_DEPENDENCY_BYTES,&length);
  if (status==MP_CODEC_OK) {
    written=mp_dependency_import_call(input,length,response,MP_MAX_DEPENDENCY_BYTES);
    if (written<=0 || (uint32_t)written>MP_MAX_DEPENDENCY_BYTES) status=MP_TRANSPORT_FAILURE;
    else status=mp_dependency_response_decode(response,(uint32_t)written,input,length,out);
  }
  free(input); free(response); return status;
}
#ifdef __cplusplus
}
#endif
#endif
#endif
