#ifndef MORROW_CHANNEL_V1_H
#define MORROW_CHANNEL_V1_H
#include "morrow_plugin_codec.h"
#include "morrow_plugin_sdk.h"
#ifdef __cplusplus
extern "C" {
#endif
#define MP_CHANNEL_VERSION 1u
#define MP_MAX_CHANNEL_WIRE_BYTES 131072u
#define MP_MAX_CHANNEL_PAYLOAD_BYTES 65536u
#define MP_MAX_CHANNEL_CURSOR_BYTES 256u
#define MP_CHANNEL_RECEIVE 1u
#define MP_CHANNEL_ACK 2u
#define MP_CHANNEL_SEND 3u
#define MP_CHANNEL_CLOSE 4u
#define MP_CHANNEL_QUERY 5u
#define MP_CHANNEL_READY 0u
#define MP_CHANNEL_FRAME 1u
#define MP_CHANNEL_ACKED 2u
#define MP_CHANNEL_ACCEPTED 3u
#define MP_CHANNEL_IDLE 4u
#define MP_CHANNEL_CLOSED 5u
#define MP_CHANNEL_CLOSING_UNCONFIRMED 6u
#define MP_CHANNEL_REVOKED 7u
#define MP_CHANNEL_EXPIRED 8u
#define MP_CHANNEL_LIMIT 9u
#define MP_CHANNEL_INVALID 10u
#define MP_CHANNEL_UNKNOWN 11u
#define MP_CHANNEL_UNSUPPORTED 12u
/* Independent additive API. Descriptor pointers must be aligned for their
 * declared native type and provide eight readable prefix bytes. A compatible
 * version/size advertises a readable full descriptor; short prefixes are rejected
 * before any full descriptor read. All spans remain live during the call.
 * Pointers never enter IPC. References are exactly 32 bytes and carry no grant. */
typedef struct mp_channel_request_v1 {
  uint32_t abi_version,struct_size,kind;
  mp_span call_id,reference,source_epoch;
  uint64_t sequence;
  uint32_t credit_bytes;
  mp_span frame_sha256,cursor,bytes;
} mp_channel_request_v1;
typedef struct mp_channel_response mp_channel_response;
/* Views expire at response_free. ACK records cursor receipt only; Accepted
 * records send admission, not peer observation or business success. */
typedef struct mp_channel_response_view {
  uint32_t status,has_frame,resource_reclaimed;
  uint64_t sequence,last_acked,accepted_sequence;
  mp_span call_id,reference,source_epoch,bytes,cursor,frame_sha256,encoded_frame;
} mp_channel_response_view;
typedef struct mp_channel_host_v1 {
  uint32_t abi_version,struct_size;
  void *context;
  mp_exchange_fn call;
} mp_channel_host_v1;
/* Pure codec: copied descriptor/spans, bounded allocation, no transport. */
uint32_t mp_channel_request_encode(const mp_channel_request_v1*,uint8_t*,uint32_t,uint32_t*);
uint32_t mp_channel_request_validate(const uint8_t*,uint32_t);
uint32_t mp_channel_schema_digest(uint8_t*,uint32_t);
uint32_t mp_channel_response_decode(const uint8_t*,uint32_t,const uint8_t*,uint32_t,mp_channel_response**);
uint32_t mp_channel_response_get(const mp_channel_response*,mp_channel_response_view*,uint32_t);
void mp_channel_response_free(mp_channel_response*);
typedef struct mp_channel_budget_v1 {
  uint32_t max_channels,max_frame_bytes;
  uint64_t max_bytes,max_messages,max_requests,max_duration_ms;
} mp_channel_budget_v1;
typedef struct mp_channel_endpoint_view {
  mp_span reference,source_epoch;
  uint32_t kind; /* 0 byteStream, 1 events */
  mp_channel_budget_v1 budget;
} mp_channel_endpoint_view;
typedef struct mp_channel_directory mp_channel_directory;
typedef struct mp_channel_directory_view {
  mp_span scope_sha256;
  const mp_channel_endpoint_view *channels;
  uint32_t channel_count;
} mp_channel_directory_view;
/* Fixed-schema metadata only, never approval. Decode copies input, validates
 * <=8 unique channels and budgets; all view pointers expire at directory_free. */
uint32_t mp_channel_directory_decode(const uint8_t*,uint32_t,mp_channel_directory**);
uint32_t mp_channel_directory_get(const mp_channel_directory*,mp_channel_directory_view*,uint32_t);
void mp_channel_directory_free(mp_channel_directory*);
/* Copy request and reserve both full-size buffers before a single submission.
 * Input/output may alias; control slots must be disjoint. Failure means no
 * usable reply, never rollback. Callback must not retain, reenter or unwind. */
uint32_t mp_channel_exchange(const mp_channel_host_v1*,const uint8_t*,uint32_t,uint8_t*,uint32_t,uint32_t*);
uint32_t mp_channel_call(const mp_channel_host_v1*,const mp_channel_request_v1*,mp_channel_response**);
/* Streaming digest keeps a fixed-size state; no transcript is retained. */
typedef struct mp_channel_digest mp_channel_digest;
uint32_t mp_channel_digest_new(mp_channel_digest**);
uint32_t mp_channel_digest_update(mp_channel_digest*,const uint8_t*,uint32_t);
uint32_t mp_channel_digest_finish(const mp_channel_digest*,uint8_t*,uint32_t);
void mp_channel_digest_free(mp_channel_digest*);
#ifdef __cplusplus
}
#endif
#if defined(__wasm32__)
#include <stdlib.h>
#ifdef __cplusplus
extern "C" {
#endif
__attribute__((import_module("morrow_channel_v1"),import_name("call")))
int32_t mp_channel_import_call(const uint8_t*,uint32_t,uint8_t*,uint32_t);
static inline uint32_t mp_channel_wasm_adapter(void *ctx,const uint8_t *input,uint32_t length,uint8_t *output,uint32_t capacity,uint32_t *written) {
  (void)ctx; int32_t n=mp_channel_import_call(input,length,output,capacity);
  if(n<=0 || (uint32_t)n>capacity)return MP_TRANSPORT_FAILURE;
  *written=(uint32_t)n;return MP_OK;
}
static inline uint32_t mp_wasm_channel_call(const mp_channel_request_v1 *request,mp_channel_response **out) {
  mp_channel_host_v1 host={MP_CHANNEL_VERSION,sizeof(mp_channel_host_v1),NULL,mp_channel_wasm_adapter};
  return mp_channel_call(&host,request,out);
}
#ifdef __cplusplus
}
#endif
#endif
#endif
