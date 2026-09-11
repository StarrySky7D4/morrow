#ifndef MORROW_PLUGIN_TASK_H
#define MORROW_PLUGIN_TASK_H
#include "morrow_plugin_codec.h"
#ifdef __cplusplus
extern "C" {
#endif
#define MP_MAX_TASK_BYTES 131072u
typedef struct mp_task mp_task;
/* Read-only spans owned by the task. IDs are correlation, never capabilities. */
typedef struct mp_task_view { mp_span task_id, command; mp_request_v1 request; } mp_task_view;
/* Same disjoint buffer and live-pointer requirements as the codec API. */
uint32_t mp_task_decode(const uint8_t*,uint32_t,mp_task**);
uint32_t mp_task_get(const mp_task*,mp_task_view*,uint32_t);
uint32_t mp_task_complete(const mp_task*,const uint8_t*,uint32_t,uint8_t*,uint32_t,uint32_t*);
void mp_task_free(mp_task*);
/* Guest ABI v2 only. Read input once, publish completion once, no calls afterward. */
int32_t mp_wasm_task_read(uint8_t*,uint32_t);
int32_t mp_wasm_task_complete(const uint8_t*,uint32_t);
#ifdef __cplusplus
}
#endif
#endif
