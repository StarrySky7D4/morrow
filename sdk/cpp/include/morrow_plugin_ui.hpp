#ifndef MORROW_PLUGIN_UI_HPP
#define MORROW_PLUGIN_UI_HPP
#include "morrow_plugin_codec.hpp"
#include "morrow_plugin_ui.h"
namespace morrow::ui {
struct node {
  std::string id, parent, label, text, action;
  uint32_t kind = MP_UI_COLUMN, tone = MP_UI_NORMAL;
  bool enabled = true, checked = false;
  uint32_t max_bytes = 0;
  node(std::string id_, std::string parent_, uint32_t kind_)
      : id(std::move(id_)), parent(std::move(parent_)), kind(kind_) {}
};
struct encoded_document {
  uint32_t status;
  std::vector<uint8_t> bytes;
};
inline encoded_document encode(const std::vector<node> &nodes) {
  encoded_document out{MP_CODEC_LIMIT, {}};
  if (nodes.empty() || nodes.size() > MP_UI_MAX_NODES)
    return out;
  std::vector<mp_ui_node_v1> raw;
  for (const auto &n : nodes) {
    if (n.id.size() > 256 || n.parent.size() > 256 || n.label.size() > 512 ||
        n.text.size() > 4096 || n.action.size() > 256)
      return out;
    const auto span = [](const std::string &s) {
      return mp_span{reinterpret_cast<const uint8_t *>(s.data()),
                     static_cast<uint32_t>(s.size())};
    };
    raw.push_back({span(n.id), span(n.parent), span(n.label), span(n.text),
                   span(n.action), n.kind, n.tone, n.enabled ? 1u : 0u,
                   n.checked ? 1u : 0u, n.max_bytes});
  }
  out.bytes.resize(MP_UI_MAX_BYTES);
  uint32_t length = 0;
  out.status = mp_ui_document_encode(
      1, sizeof(mp_ui_node_v1), raw.data(), static_cast<uint32_t>(raw.size()),
      out.bytes.data(), MP_UI_MAX_BYTES, &length);
  out.bytes.resize(out.status == MP_CODEC_OK ? length : 0);
  return out;
}
class event {
  mp_ui_event *value_ = nullptr;
  uint32_t status_ = MP_CODEC_INVALID;

public:
  event() = default;
  ~event() { mp_ui_event_free(value_); }
  event(const event &) = delete;
  event &operator=(const event &) = delete;
  event(event &&other) noexcept
      : value_(std::exchange(other.value_, nullptr)),
        status_(std::exchange(other.status_, MP_CODEC_INVALID)) {}
  event &operator=(event &&other) noexcept {
    if (this != &other) {
      mp_ui_event_free(value_);
      value_ = std::exchange(other.value_, nullptr);
      status_ = std::exchange(other.status_, MP_CODEC_INVALID);
    }
    return *this;
  }
  static event decode(const std::vector<uint8_t> &bytes) {
    event e;
    if (bytes.size() > MP_UI_MAX_BYTES) {
      e.status_ = MP_CODEC_LIMIT;
      return e;
    }
    e.status_ = mp_ui_event_decode(
        bytes.data(), static_cast<uint32_t>(bytes.size()), &e.value_);
    return e;
  }
  uint32_t status() const { return status_; }
  mp_ui_event_view view() const {
    mp_ui_event_view v{};
    if (status_ != MP_CODEC_OK ||
        mp_ui_event_get(value_, &v, sizeof(v)) != MP_CODEC_OK)
      detail::codec_logic_error();
    return v;
  }
};
} // namespace morrow::ui
#endif
