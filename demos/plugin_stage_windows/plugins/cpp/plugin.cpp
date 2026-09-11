#include "../stage_common.h"
#include "morrow_plugin_ui.hpp"
#include <algorithm>
#include <string_view>
static std::string text(mp_span s) {
  return s.length ? std::string((const char *)s.data, s.length) : std::string();
}
struct item {
  std::string title;
  bool done = false;
};
static std::vector<item> parse(const std::string &source) {
  std::vector<item> items;
  size_t start = 0;
  while (start <= source.size() && items.size() < 8) {
    size_t end = source.find('\n', start);
    if (end == std::string::npos)
      end = source.size();
    auto line = source.substr(start, end - start);
    auto first = line.find_first_not_of(" \t");
    auto last = line.find_last_not_of(" \t");
    if (first != std::string::npos)
      items.push_back({line.substr(first, last - first + 1), false});
    if (end == source.size())
      break;
    start = end + 1;
  }
  return items;
}
static std::string join(const std::vector<item> &items) {
  std::string s;
  for (const auto &i : items) {
    if (!s.empty())
      s += '\n';
    s += i.title;
  }
  return s;
}
extern "C" int32_t morrow_run() {
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES);
  stage_input s{};
  struct cleanup {
    stage_input *s;
    ~cleanup() { stage_close(s); }
  } cleanup{&s};
  if (!stage_open(&s, input.data(), (uint32_t)input.size()))
    return -1;
  const std::string sample = "收集灵感\n整理参考资料\n制作一张卡片";
  std::string source = sample;
  bool hide = false;
  if (s.update) {
    source = text(stage_find(&s, "title").text);
    hide = stage_find(&s, "option").checked;
  }
  auto items = parse(source);
  if (s.update) {
    for (size_t i = 0; i < items.size(); i++)
      items[i].done =
          stage_find(&s, ("item" + std::to_string(i)).c_str()).checked;
    if (stage_action(&s, "title", "edit", MP_UI_EDIT_TEXT)) {
      source = text(s.e.text);
      items = parse(source);
    } else if (stage_action(&s, "option", "hide", MP_UI_SET_TOGGLE))
      hide = s.e.checked;
    else if (stage_action(&s, "sort", "sort", MP_UI_ACTIVATE)) {
      std::stable_sort(
          items.begin(), items.end(),
          [](const item &a, const item &b) { return a.title < b.title; });
      source = join(items);
    } else if (stage_action(&s, "clear", "clear", MP_UI_ACTIVATE)) {
      items.erase(std::remove_if(items.begin(), items.end(),
                                 [](const item &i) { return i.done; }),
                  items.end());
      source = join(items);
    } else if (stage_action(&s, "reset", "reset", MP_UI_ACTIVATE)) {
      source = sample;
      hide = false;
      items = parse(source);
    } else {
      bool found = false;
      for (size_t i = 0; i < items.size(); i++) {
        auto id = "item" + std::to_string(i);
        if (stage_action(&s, id.c_str(), id.c_str(), MP_UI_SET_TOGGLE)) {
          items[i].done = s.e.checked;
          found = true;
        }
      }
      if (!found)
        return -1;
    }
  }
  using morrow::ui::node;
  std::vector<node> nodes{{"root", "", MP_UI_COLUMN},
                          {"title", "root", MP_UI_TEXT_INPUT},
                          {"option", "root", MP_UI_TOGGLE}};
  nodes[1].label = "每行一个任务（展示前 8 项）";
  nodes[1].text = source;
  nodes[1].max_bytes = 1024;
  nodes[1].action = "edit";
  nodes[2].label = "预览只看未完成";
  nodes[2].action = "hide";
  nodes[2].checked = hide;
  size_t done = 0;
  std::string result;
  for (size_t i = 0; i < items.size(); i++) {
    auto id = "item" + std::to_string(i);
    node n(id, "root", MP_UI_TOGGLE);
    n.label = items[i].title;
    n.action = id;
    n.checked = items[i].done;
    // Per-label protocol limit, preserving UTF-8 boundaries.
    if (n.label.size() > 480) {
      n.label.resize(480);
      while (!n.label.empty() && ((unsigned char)n.label.back() & 0xc0) == 0x80)
        n.label.pop_back();
      if (!n.label.empty() && (unsigned char)n.label.back() >= 0xc0)
        n.label.pop_back();
      n.label += "…";
    }
    nodes.push_back(n);
    done += items[i].done;
    if (!hide || !items[i].done) {
      if (!result.empty())
        result += '\n';
      result += (items[i].done ? "✓ " : "○ ") + items[i].title;
    }
  }
  nodes.emplace_back("actions", "root", MP_UI_ROW);
  for (auto entry : {std::pair{"sort", "按名称排序"},
                     {"clear", "清除已完成"},
                     {"reset", "恢复示例"}}) {
    node n(entry.first, "actions", MP_UI_BUTTON);
    n.action = entry.first;
    n.label = entry.second;
    n.enabled = std::string(entry.first) != "clear" || done > 0;
    nodes.push_back(n);
  }
  for (auto entry :
       {std::pair{"result",
                  result.empty()
                      ? (hide && done > 0 ? std::string("全部完成，做得不错！")
                                          : std::string("还没有任务"))
                      : result},
        {"detail",
         "已完成 " + std::to_string(done) + " / " +
             std::to_string(items.size()) + " · " +
             std::to_string(items.empty() ? 0 : done * 100 / items.size()) +
             "%"},
        {"caption",
         std::string("编辑清单会重置勾选；排序按字符顺序并保留勾选，不使用拼音"
                     "排序。排序与清除会将输入整理为展示的前 8 项。")}}) {
    node n(entry.first, "root", MP_UI_TEXT);
    n.text = entry.second;
    nodes.push_back(n);
  }
  auto document = morrow::ui::encode(nodes);
  if (document.status != MP_CODEC_OK)
    return -1;
  std::vector<uint8_t> completion(MP_MAX_TASK_BYTES);
  uint32_t size = 0;
  if (mp_task_output(s.task, document.bytes.data(),
                     (uint32_t)document.bytes.size(), completion.data(),
                     (uint32_t)completion.size(), &size) != MP_CODEC_OK)
    return -1;
  return mp_wasm_task_complete(completion.data(), size);
}
