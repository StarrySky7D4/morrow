#ifndef MORROW_PLUGIN_SERVICE_H
#define MORROW_PLUGIN_SERVICE_H
#include "morrow_plugin_codec.h"
#include "morrow_plugin_sdk.h"
#include "morrow_plugin_task.h"
#ifdef __cplusplus
extern "C" {
#endif
#define MP_SERVICE_ABI_VERSION 1u
#define MP_MAX_SERVICE_FRAME_BYTES 131072u
#define MP_MAX_SERVICE_BODY_BYTES 65536u
#define MP_MAX_SERVICE_HEADERS 64u
typedef struct mp_service_header { mp_span name, value; } mp_service_header;
typedef struct mp_service_request mp_service_request;
/* SDK-owned spans expire at request_free. principal is a trusted host assertion,
 * never a guest grant or authentication based on bytes supplied externally. */
typedef struct mp_service_request_view {
  uint64_t call_id;
  mp_span service, handler, principal, method, target, body;
  const mp_service_header *headers;
  uint32_t header_count;
} mp_service_request_view;
typedef struct mp_service_reply_v1 {
  uint32_t abi_version, struct_size, status;
  mp_span body;
  const mp_service_header *headers;
  uint32_t header_count;
} mp_service_reply_v1;
/* Pointers must be live, aligned and disjoint as applicable. Decode copies the
 * exact input; out is NULL on failure. Encoding uses that original frame hash,
 * not a reconstructed request. Failed encode leaves length zero/output untouched. */
uint32_t mp_service_request_decode(const uint8_t*,uint32_t,mp_service_request**);
uint32_t mp_service_request_get(const mp_service_request*,mp_service_request_view*,uint32_t);
uint32_t mp_service_response_encode(const mp_service_request*,const mp_service_reply_v1*,uint8_t*,uint32_t,uint32_t*);
void mp_service_request_free(mp_service_request*);
/* Independently owned optional metadata. Never grants IO authority. */
typedef struct mp_service_resources mp_service_resources;
typedef struct mp_service_endpoint {
  mp_span reference, credential;
  const mp_span *methods;
  uint32_t method_count;
  uint64_t max_request_bytes, max_response_bytes, timeout_ms, response_frame_limit;
} mp_service_endpoint;
typedef struct mp_service_resources_view {
  mp_span scope_sha256;
  const mp_service_endpoint *endpoints;
  uint32_t endpoint_count;
} mp_service_resources_view;
/* OK + NULL means absent. Failure sets out NULL. Views live until resources_free,
 * independently of the request lifetime; malformed/duplicate headers fail closed. */
uint32_t mp_service_request_resources(const mp_service_request*,mp_service_resources**);
uint32_t mp_service_resources_get(const mp_service_resources*,mp_service_resources_view*,uint32_t);
void mp_service_resources_free(mp_service_resources*);
#ifdef __cplusplus
}
#endif
#if defined(__wasm32__)
#include <stdlib.h>
/* Service frames use the existing read/complete imports, not morrow_io_v1.call.
 * Read once, complete once, no automatic retries or calls after completion. */
static inline uint32_t mp_wasm_service_read(mp_service_request **out) {
  uint8_t *input; int32_t length; uint32_t status;
  if (!out) return MP_CODEC_INVALID;
  *out=NULL;
  input=(uint8_t*)malloc(MP_MAX_SERVICE_FRAME_BYTES);
  if (!input) return MP_NO_MEMORY;
  length=mp_wasm_task_read(input,MP_MAX_SERVICE_FRAME_BYTES);
  status=(length<=0 || (uint32_t)length>MP_MAX_SERVICE_FRAME_BYTES)
      ? MP_TRANSPORT_FAILURE : mp_service_request_decode(input,(uint32_t)length,out);
  free(input); return status;
}
static inline uint32_t mp_wasm_service_complete(const mp_service_request *request,const mp_service_reply_v1 *reply) {
  uint8_t *output=(uint8_t*)malloc(MP_MAX_SERVICE_FRAME_BYTES);
  uint32_t length=0, status;
  if (!output) return MP_NO_MEMORY;
  status=mp_service_response_encode(request,reply,output,MP_MAX_SERVICE_FRAME_BYTES,&length);
  if (status==MP_CODEC_OK && mp_wasm_task_complete(output,length)!=0) status=MP_TRANSPORT_FAILURE;
  free(output); return status;
}
#endif
#endif
