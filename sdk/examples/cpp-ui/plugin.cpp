#include "morrow_plugin_task.hpp"
#include "morrow_plugin_ui.hpp"
#include <string_view>
static std::string_view text(mp_span s) {
  return {reinterpret_cast<const char *>(s.data), s.length};
}
extern "C" int32_t morrow_run() {
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES);
  int32_t n = mp_wasm_task_read(input.data(), MP_MAX_TASK_BYTES);
  if (n <= 0 || n > (int32_t)input.size())
    return -1;
  input.resize(static_cast<size_t>(n));
  auto task = morrow::task::decode(input);
  if (task.status() != MP_CODEC_OK)
    return -1;
  auto t = task.transform();
  if (text(t.output_type) != "morrow.ui.document.v1")
    return -1;
  const auto fail = [&]() {
    auto f = task.fail(MP_TASK_INVALID_INPUT, "Invalid form input");
    return f.status == MP_CODEC_OK
               ? mp_wasm_task_complete(f.bytes.data(),
                                       static_cast<uint32_t>(f.bytes.size()))
               : -1;
  };
  std::string title;
  if (text(t.handler) == "ui.form" && text(t.input_type) == "text.utf8")
    title = std::string(text(t.input));
  else if (text(t.handler) == "ui.edit" &&
           text(t.input_type) == "morrow.ui.event.v1") {
    std::vector<uint8_t> bytes;
    if (t.input.length)
      bytes.assign(t.input.data, t.input.data + t.input.length);
    auto e = morrow::ui::event::decode(bytes);
    if (e.status() != MP_CODEC_OK)
      return fail();
    auto v = e.view();
    if (v.kind != MP_UI_EDIT_TEXT || text(v.node) != "title" ||
        text(v.action) != "title.edit")
      return fail();
    title = std::string(text(v.text));
  } else
    return -1;
  using morrow::ui::node;
  std::vector<node> nodes{{"root", "", MP_UI_COLUMN},
                          {"heading", "root", MP_UI_TEXT},
                          {"title", "root", MP_UI_TEXT_INPUT},
                          {"pinned", "root", MP_UI_TOGGLE},
                          {"apply", "root", MP_UI_BUTTON}};
  nodes[1].text = "插件表单";
  nodes[1].tone = MP_UI_EMPHASIS;
  nodes[2].label = "标题";
  nodes[2].text = title;
  nodes[2].action = "title.edit";
  nodes[2].max_bytes = 32;
  nodes[3].label = "置顶";
  nodes[3].action = "pin.toggle";
  nodes[4].label = "应用";
  nodes[4].action = "apply";
  auto document = morrow::ui::encode(nodes);
  if (document.status != MP_CODEC_OK)
    return fail();
  auto completion = task.output(document.bytes);
  if (completion.status != MP_CODEC_OK)
    return -1;
  return mp_wasm_task_complete(completion.bytes.data(),
                               static_cast<uint32_t>(completion.bytes.size()));
}
