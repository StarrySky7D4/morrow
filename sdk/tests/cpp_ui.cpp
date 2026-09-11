#include "morrow_plugin_ui.hpp"
#include <cassert>
#include <fstream>
#include <iostream>
#include <iterator>
int main(int argc, char **argv) {
  assert(argc == 2);
  std::ifstream f(argv[1], std::ios::binary);
  assert(f.good());
  std::vector<uint8_t> bytes{std::istreambuf_iterator<char>(f), {}};
  auto e = morrow::ui::event::decode(bytes);
  assert(e.status() == MP_CODEC_OK);
  auto owned = std::move(e);
  assert(e.status() != MP_CODEC_OK);
  auto v = owned.view();
  assert(v.generation == UINT64_MAX && v.revision == 1 && v.serial == 1 &&
         v.kind == MP_UI_EDIT_TEXT);
  assert(std::string(reinterpret_cast<const char *>(v.text.data),
                     v.text.length) == "从 Dart 编辑🌈");
  morrow::ui::event assigned;
  assigned = std::move(owned);
  assert(owned.status() != MP_CODEC_OK);
  assert(assigned.view().generation == UINT64_MAX);
  std::vector<morrow::ui::node> nodes{{"root", "", MP_UI_COLUMN},
                                      {"field", "root", MP_UI_TEXT_INPUT}};
  nodes[1].text = "中文";
  nodes[1].label = "标题";
  nodes[1].action = "edit";
  nodes[1].max_bytes = 6;
  auto result = morrow::ui::encode(nodes);
  assert(result.status == MP_CODEC_OK && !result.bytes.empty());
  auto encoded = result.bytes;
  nodes[1].text = "too-long";
  result = morrow::ui::encode(nodes);
  assert(result.status != MP_CODEC_OK && result.bytes.empty());
  assert(!encoded.empty());
  nodes[1].text = "";
  nodes[1].parent = "field";
  assert(morrow::ui::encode(nodes).status != MP_CODEC_OK);
  auto malformed = morrow::ui::event::decode({0});
  assert(malformed.status() != MP_CODEC_OK);
  std::cout << "PASS: native C++ UI owning strings, move-only event handles, "
               "exact UInt64, Unicode and invalid input rejection\n";
}
