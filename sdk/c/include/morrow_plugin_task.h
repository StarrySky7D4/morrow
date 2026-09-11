#ifndef MORROW_PLUGIN_TASK_H
#define MORROW_PLUGIN_TASK_H
#include "morrow_plugin_codec.h"
#ifdef __cplusplus
extern "C" {
#endif
#define MP_MAX_TASK_FAILURE_MESSAGE_BYTES 1024u
#define MP_TASK_INVALID_INPUT 0u
#define MP_TASK_UNSUPPORTED_INPUT 1u
#define MP_TASK_RESOURCE_LIMIT 2u
#define MP_TASK_FAILED 3u
#define MP_MAX_TASK_BYTES 131072u
#define MP_MAX_TASK_VALUE_BYTES 65536u
typedef struct mp_transform_view {mp_span handler,input_type,output_type,input;} mp_transform_view;
typedef struct mp_task mp_task;
/* Read-only spans owned by the task. IDs are correlation, never capabilities. */
typedef struct mp_task_view { mp_span task_id, command; mp_request_v1 request; } mp_task_view;
/* Same disjoint buffer and live-pointer requirements as the codec API. */
uint32_t mp_task_decode(const uint8_t*,uint32_t,mp_task**);
uint32_t mp_task_get(const mp_task*,mp_task_view*,uint32_t);
uint32_t mp_task_complete(const mp_task*,const uint8_t*,uint32_t,uint8_t*,uint32_t,uint32_t*);
uint32_t mp_task_get_transform(const mp_task*,mp_transform_view*,uint32_t);
uint32_t mp_task_output(const mp_task*,const uint8_t*,uint32_t,uint8_t*,uint32_t,uint32_t*);
/* Nonempty UTF-8 plain text, no control characters. A failure is not a core receipt. */
uint32_t mp_task_fail(const mp_task*,uint32_t,const uint8_t*,uint32_t,uint8_t*,uint32_t,uint32_t*);
void mp_task_free(mp_task*);
/* Guest ABI v2 only. Read input once, publish completion once, no calls afterward. */
int32_t mp_wasm_task_read(uint8_t*,uint32_t);
int32_t mp_wasm_task_complete(const uint8_t*,uint32_t);
#ifdef __cplusplus
}
#endif
#endif
