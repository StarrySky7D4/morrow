#include "morrow_plugin_task.h"
#include "morrow_plugin_ui.h"
#include <string.h>
static mp_span text(const char *s) {
  mp_span v = {(const uint8_t *)s, (uint32_t)strlen(s)};
  return v;
}
static int equal(mp_span a, const char *b) {
  return a.length == strlen(b) &&
         (!a.length || memcmp(a.data, b, a.length) == 0);
}
static mp_ui_node_v1 node(const char *id, const char *parent, uint32_t kind) {
  mp_ui_node_v1 n = {0};
  n.id = text(id);
  n.parent = text(parent);
  n.kind = kind;
  n.enabled = 1;
  return n;
}
int32_t morrow_run(void) {
  static uint8_t input[MP_MAX_TASK_BYTES], document[MP_UI_MAX_BYTES],
      completion[MP_MAX_TASK_BYTES];
  mp_task *task = 0;
  mp_ui_event *event = 0;
  mp_transform_view t = {0};
  mp_ui_event_view e = {0};
  uint32_t length = 0, status = MP_CODEC_INVALID;
  int32_t result = -1;
  int32_t read = mp_wasm_task_read(input, sizeof(input));
  if (read <= 0 || read > (int32_t)sizeof(input) ||
      mp_task_decode(input, (uint32_t)read, &task) != MP_CODEC_OK)
    goto done;
  if (mp_task_get_transform(task, &t, sizeof(t)) != MP_CODEC_OK ||
      !equal(t.output_type, "morrow.ui.document.v1"))
    goto done;
  mp_span title = {0};
  if (equal(t.handler, "ui.form") && equal(t.input_type, "text.utf8")) {
    title = t.input;
  } else if (equal(t.handler, "ui.edit") &&
             equal(t.input_type, "morrow.ui.event.v1")) {
    if (mp_ui_event_decode(t.input.data, t.input.length, &event) !=
            MP_CODEC_OK ||
        mp_ui_event_get(event, &e, sizeof(e)) != MP_CODEC_OK)
      goto invalid;
    if (e.kind != MP_UI_EDIT_TEXT || !equal(e.node, "title") ||
        !equal(e.action, "title.edit"))
      goto invalid;
    title = e.text;
  } else
    goto done;
  mp_ui_node_v1 nodes[5];
  nodes[0] = node("root", "", MP_UI_COLUMN);
  nodes[1] = node("heading", "root", MP_UI_TEXT);
  nodes[1].text = text("插件表单");
  nodes[1].tone = MP_UI_EMPHASIS;
  nodes[2] = node("title", "root", MP_UI_TEXT_INPUT);
  nodes[2].label = text("标题");
  nodes[2].text = title;
  nodes[2].action = text("title.edit");
  nodes[2].max_bytes = 32;
  nodes[3] = node("pinned", "root", MP_UI_TOGGLE);
  nodes[3].label = text("置顶");
  nodes[3].action = text("pin.toggle");
  nodes[4] = node("apply", "root", MP_UI_BUTTON);
  nodes[4].label = text("应用");
  nodes[4].action = text("apply");
  status = mp_ui_document_encode(1, sizeof(mp_ui_node_v1), nodes, 5, document,
                                 sizeof(document), &length);
  if (status != MP_CODEC_OK)
    goto invalid;
  status = mp_task_output(task, document, length, completion,
                          sizeof(completion), &length);
  goto publish;
invalid:
  status = mp_task_fail(task, MP_TASK_INVALID_INPUT,
                        (const uint8_t *)"Invalid form input", 18, completion,
                        sizeof(completion), &length);
publish:
  if (status == MP_CODEC_OK)
    result = mp_wasm_task_complete(completion, length);
done:
  mp_ui_event_free(event);
  mp_task_free(task);
  return result;
}
