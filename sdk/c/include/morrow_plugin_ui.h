#ifndef MORROW_PLUGIN_UI_H
#define MORROW_PLUGIN_UI_H
#include "morrow_plugin_codec.h"
#ifdef __cplusplus
extern "C" {
#endif
#define MP_UI_MAX_BYTES 65536u
#define MP_UI_MAX_NODES 128u
#define MP_UI_COLUMN 0u
#define MP_UI_ROW 1u
#define MP_UI_TEXT 2u
#define MP_UI_BUTTON 3u
#define MP_UI_TEXT_INPUT 4u
#define MP_UI_TOGGLE 5u
#define MP_UI_NORMAL 0u
#define MP_UI_MUTED 1u
#define MP_UI_EMPHASIS 2u
#define MP_UI_ACTIVATE 0u
#define MP_UI_EDIT_TEXT 1u
#define MP_UI_SET_TOGGLE 2u
/* UTF-8 spans; initialize enabled=1 explicitly, boolean fields only 0/1.
 * Parents precede children, first node is the single root column. */
typedef struct mp_ui_node_v1 {
  mp_span id, parent, label, text, action;
  uint32_t kind, tone, enabled, checked, max_bytes;
} mp_ui_node_v1;
/* Pure codec, no host access. ABI=1, node_size=sizeof(mp_ui_node_v1).
 * All pointers aligned, disjoint and live for the call. Output length is zero
 * on failure; buffer is untouched. Encoded output owns no source pointers. */
uint32_t mp_ui_document_encode(uint32_t abi, uint32_t node_size,
                               const mp_ui_node_v1 *, uint32_t count, uint8_t *,
                               uint32_t capacity, uint32_t *length);
typedef struct mp_ui_event mp_ui_event;
typedef struct mp_ui_event_view {
  mp_span view, node, action, text;
  uint64_t generation, revision, serial;
  uint32_t kind, checked;
} mp_ui_event_view;
/* Decode checks the wire contract, not authorization or current view state.
 * Host must bind the task to a validated session. Handles are local owned
 * values; spans from get expire at free. NULL output handle on every failure.
 */
uint32_t mp_ui_event_decode(const uint8_t *, uint32_t, mp_ui_event **);
uint32_t mp_ui_event_get(const mp_ui_event *, mp_ui_event_view *,
                         uint32_t view_size);
void mp_ui_event_free(mp_ui_event *);
#ifdef __cplusplus
}
#endif
#endif
