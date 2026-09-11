#ifndef MORROW_STAGE_COMMON_H
#define MORROW_STAGE_COMMON_H
#include "morrow_plugin_task.h"
#include "morrow_plugin_ui.h"
#include <string.h>
static inline mp_span stage_text(const char *s) {
  mp_span r = {(const uint8_t *)s, (uint32_t)strlen(s)};
  return r;
}
static inline int stage_equal(mp_span a, const char *b) {
  return a.length == strlen(b) &&
         (!a.length || memcmp(a.data, b, a.length) == 0);
}
static inline mp_ui_node_v1 stage_node(const char *id, const char *parent,
                                       uint32_t kind) {
  mp_ui_node_v1 n;
  memset(&n, 0, sizeof(n));
  n.id = stage_text(id);
  n.parent = stage_text(parent);
  n.kind = kind;
  n.enabled = 1;
  return n;
}
typedef struct stage_input {
  mp_task *task;
  mp_ui_document *document;
  mp_ui_event *event;
  mp_ui_event_view e;
  uint32_t count;
  int update;
} stage_input;
static inline void stage_close(stage_input *s) {
  mp_ui_event_free(s->event);
  mp_ui_document_free(s->document);
  mp_task_free(s->task);
}
static inline int stage_open(stage_input *s, uint8_t *buffer,
                             uint32_t capacity) {
  mp_transform_view t;
  memset(&t, 0, sizeof(t));
  int32_t n = mp_wasm_task_read(buffer, capacity);
  if (n <= 0 || (uint32_t)n > capacity ||
      mp_task_decode(buffer, (uint32_t)n, &s->task) != MP_CODEC_OK)
    return 0;
  if (mp_task_get_transform(s->task, &t, sizeof(t)) != MP_CODEC_OK ||
      !stage_equal(t.output_type, "morrow.ui.document.v1"))
    return 0;
  if (stage_equal(t.handler, "demo.open") &&
      stage_equal(t.input_type, "text.utf8") && t.input.length == 0)
    return 1;
  if (!stage_equal(t.handler, "demo.update") ||
      !stage_equal(t.input_type, "morrow.demo.update.v1") || t.input.length < 8)
    return 0;
  uint32_t size = (uint32_t)t.input.data[0] | ((uint32_t)t.input.data[1] << 8) |
                  ((uint32_t)t.input.data[2] << 16) |
                  ((uint32_t)t.input.data[3] << 24);
  if (size > t.input.length - 8 || t.input.data[4] || t.input.data[5] ||
      t.input.data[6] || t.input.data[7])
    return 0;
  if (mp_ui_document_decode(t.input.data + 8, size, &s->document, &s->count) !=
          MP_CODEC_OK ||
      mp_ui_event_decode(t.input.data + 8 + size, t.input.length - 8 - size,
                         &s->event) != MP_CODEC_OK ||
      mp_ui_event_get(s->event, &s->e, sizeof(s->e)) != MP_CODEC_OK)
    return 0;
  s->update = 1;
  return 1;
}
static inline mp_ui_node_v1 stage_find(stage_input *s, const char *id) {
  mp_ui_node_v1 n;
  memset(&n, 0, sizeof(n));
  for (uint32_t i = 0; i < s->count; i++)
    if (mp_ui_document_node(s->document, i, &n, sizeof(n)) == MP_CODEC_OK &&
        stage_equal(n.id, id))
      return n;
  memset(&n, 0, sizeof(n));
  return n;
}
static inline int stage_action(stage_input *s, const char *id,
                               const char *action, uint32_t kind) {
  return s->e.kind == kind && stage_equal(s->e.node, id) &&
         stage_equal(s->e.action, action);
}
#endif
