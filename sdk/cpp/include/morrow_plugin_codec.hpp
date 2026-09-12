#ifndef MORROW_PLUGIN_CODEC_HPP
#define MORROW_PLUGIN_CODEC_HPP
#include "morrow_plugin_codec.h"
#include "morrow_plugin_sdk.h"
#include <cstdlib>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
namespace morrow {
namespace detail {
[[noreturn]] inline void codec_length_error() {
#if defined(__cpp_exceptions) || defined(_CPPUNWIND)
  throw std::length_error("SDK text exceeds ABI length");
#else
  std::abort();
#endif
}
[[noreturn]] inline void codec_logic_error() {
#if defined(__cpp_exceptions) || defined(_CPPUNWIND)
  throw std::logic_error("SDK reply has no decoded value");
#else
  std::abort();
#endif
}
} // namespace detail

struct encoded_request {
  uint32_t status;
  std::vector<uint8_t> bytes;
};
class decoded_reply;
// Own strings; borrowed C spans are constructed only for a synchronous call.
class request {
  uint32_t kind_;
  std::string id_, card_, extra_;
  uint64_t revision_ = 0, offset_ = 0;
  uint32_t length_ = 0;
  request(uint32_t kind, std::string id, std::string card,
          std::string extra = {}, uint64_t revision = 0, uint64_t offset = 0,
          uint32_t length = 0)
      : kind_(kind), id_(std::move(id)), card_(std::move(card)),
        extra_(std::move(extra)), revision_(revision), offset_(offset),
        length_(length) {}
  static mp_span span(const std::string &s) {
    if (s.size() > std::numeric_limits<uint32_t>::max())
      detail::codec_length_error();
    return {reinterpret_cast<const uint8_t *>(s.data()),
            static_cast<uint32_t>(s.size())};
  }
  mp_request_v1 descriptor() const {
    mp_request_v1 r{};
    r.abi_version = 1;
    r.struct_size = sizeof(r);
    r.kind = kind_;
    r.request_id = span(id_);
    r.card_id = span(card_);
    if (kind_ == MP_REQUEST_RENAME)
      r.title = span(extra_);
    if (kind_ == MP_REQUEST_QUERY)
      r.operation_id = span(extra_);
    if (kind_ == MP_REQUEST_ATTACHMENT)
      r.attachment_id = span(extra_);
    r.revision = revision_;
    r.offset = offset_;
    r.length = length_;
    return r;
  }
  friend class decoded_reply;

public:
  static request rename(std::string id, std::string card, uint64_t revision,
                        std::string title) {
    return request(MP_REQUEST_RENAME, std::move(id), std::move(card),
                   std::move(title), revision);
  }
  static request summary(std::string id, std::string card) {
    return request(MP_REQUEST_SUMMARY, std::move(id), std::move(card));
  }
  static request query(std::string id, std::string card,
                       std::string operation) {
    return request(MP_REQUEST_QUERY, std::move(id), std::move(card),
                   std::move(operation));
  }
  static request attachment(std::string id, std::string card,
                            std::string attachment, uint64_t revision,
                            uint64_t offset, uint32_t length) {
    return request(MP_REQUEST_ATTACHMENT, std::move(id), std::move(card),
                   std::move(attachment), revision, offset, length);
  }
  encoded_request encode() const {
    auto r = descriptor();
    encoded_request out{MP_CODEC_OK,
                        std::vector<uint8_t>(MP_MAX_MESSAGE_BYTES)};
    uint32_t length = 0;
    out.status =
        mp_request_encode(&r, out.bytes.data(), MP_MAX_MESSAGE_BYTES, &length);
    out.bytes.resize(out.status == MP_CODEC_OK ? length : 0);
    return out;
  }
};
// Owns both text and binary body; no foreign spans escape encode/decode calls.
class content_request {
  uint32_t kind_ = 0, format_ = 0, length_ = 0;
  uint64_t revision_ = 0, offset_ = 0;
  std::string id_, card_, type_, title_, preview_;
  std::vector<uint8_t> body_;
  static mp_span span(const std::string& text) {
    if (text.size() > UINT32_MAX) detail::codec_length_error();
    return {reinterpret_cast<const uint8_t*>(text.data()), static_cast<uint32_t>(text.size())};
  }
  mp_content_request_v1 descriptor() const {
    if (body_.size() > UINT32_MAX) detail::codec_length_error();
    mp_content_request_v1 v{}; v.abi_version = 1; v.struct_size = sizeof(v);
    v.kind = kind_; v.format_version = format_; v.request_id = span(id_); v.card_id = span(card_);
    v.type_id = span(type_); v.title = span(title_); v.preview = span(preview_);
    v.body = {body_.data(), static_cast<uint32_t>(body_.size())};
    v.revision = revision_; v.offset = offset_; v.length = length_; return v;
  }
  friend class decoded_reply;
 public:
  static content_request create(std::string id, std::string card, std::string type,
      uint32_t format, std::string title, std::vector<uint8_t> body) {
    content_request v; v.kind_ = MP_CONTENT_CREATE; v.id_ = std::move(id); v.card_ = std::move(card);
    v.type_ = std::move(type); v.format_ = format; v.title_ = std::move(title); v.body_ = std::move(body); return v;
  }
  static content_request edit(std::string id, std::string card, uint64_t revision,
      std::string title, std::vector<uint8_t> body, std::string preview) {
    content_request v; v.kind_ = MP_CONTENT_EDIT; v.id_ = std::move(id); v.card_ = std::move(card);
    v.revision_ = revision; v.title_ = std::move(title); v.body_ = std::move(body); v.preview_ = std::move(preview); return v;
  }
  static content_request read(std::string id, std::string card, uint64_t revision, uint64_t offset, uint32_t length) {
    content_request v; v.kind_ = MP_CONTENT_READ; v.id_ = std::move(id); v.card_ = std::move(card);
    v.revision_ = revision; v.offset_ = offset; v.length_ = length; return v;
  }
  encoded_request encode() const {
    auto v = descriptor(); encoded_request out{MP_CODEC_OK, std::vector<uint8_t>(MP_MAX_MESSAGE_BYTES)};
    uint32_t length = 0; out.status = mp_content_request_encode(&v, out.bytes.data(), MP_MAX_MESSAGE_BYTES, &length);
    out.bytes.resize(out.status == MP_CODEC_OK ? length : 0); return out;
  }
};
// Move-only reply owner. view() spans remain borrowed until this owner is
// destroyed or replaced; copy fields to retain them independently. No automatic
// host retry.
class decoded_reply {
  mp_reply *handle_ = nullptr;
  uint32_t status_ = MP_CODEC_INVALID;

public:
  decoded_reply() = default;
  ~decoded_reply() { mp_reply_free(handle_); }
  decoded_reply(const decoded_reply &) = delete;
  decoded_reply &operator=(const decoded_reply &) = delete;
  decoded_reply(decoded_reply &&v) noexcept
      : handle_(std::exchange(v.handle_, nullptr)),
        status_(std::exchange(v.status_, MP_CODEC_INVALID)) {}
  decoded_reply &operator=(decoded_reply &&v) noexcept {
    if (this != &v) {
      mp_reply_free(handle_);
      handle_ = std::exchange(v.handle_, nullptr);
      status_ = std::exchange(v.status_, MP_CODEC_INVALID);
    }
    return *this;
  }
  static decoded_reply decode(const request &expected,
                              const std::vector<uint8_t> &bytes) {
    decoded_reply result;
    if (bytes.size() > MP_MAX_MESSAGE_BYTES) {
      result.status_ = MP_CODEC_LIMIT;
      return result;
    }
    auto r = expected.descriptor();
    result.status_ = mp_reply_decode(
        bytes.data(), static_cast<uint32_t>(bytes.size()), &r, &result.handle_);
    return result;
  }
  static decoded_reply decode(const content_request &expected, const std::vector<uint8_t>& bytes) {
    decoded_reply result;
    if (bytes.size() > MP_MAX_MESSAGE_BYTES) { result.status_ = MP_CODEC_LIMIT; return result; }
    auto v = expected.descriptor();
    result.status_ = mp_content_reply_decode(bytes.data(), static_cast<uint32_t>(bytes.size()), &v, &result.handle_);
    return result;
  }
  uint32_t status() const { return status_; }
  mp_reply_view view() const {
    mp_reply_view v{};
    if (status_ != MP_CODEC_OK ||
        mp_reply_get(handle_, &v, sizeof(v)) != MP_CODEC_OK)
      detail::codec_logic_error();
    return v;
  }
};
} // namespace morrow
#endif
