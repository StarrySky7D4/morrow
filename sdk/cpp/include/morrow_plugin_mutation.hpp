#ifndef MORROW_PLUGIN_MUTATION_HPP
#define MORROW_PLUGIN_MUTATION_HPP

#include "morrow_plugin_mutation.h"
#include "morrow_plugin_codec.hpp"
#include <array>
#include <cstdint>
#include <limits>
#include <string>
#include <utility>
#include <vector>

namespace morrow {

// Owns every descriptor byte until the synchronous pure C encode returns.
class mutation_request {
  uint32_t kind_;
  uint64_t call_id_;
  std::array<uint8_t, 32> reference_, submission_, content_sha256_{};
  std::string operation_id_;
  uint32_t deadline_ms_;
  uint64_t content_length_ = 0, offset_ = 0;
  std::vector<uint8_t> bytes_;

  mutation_request(uint32_t kind, uint64_t call_id,
                   std::array<uint8_t, 32> reference,
                   std::array<uint8_t, 32> submission,
                   std::string operation_id, uint32_t deadline_ms)
      : kind_(kind), call_id_(call_id), reference_(reference),
        submission_(submission), operation_id_(std::move(operation_id)),
        deadline_ms_(deadline_ms) {}

  static mp_span span(const std::array<uint8_t, 32>& value) noexcept {
    return {value.data(), static_cast<uint32_t>(value.size())};
  }
  static mp_span span(const std::string& value) noexcept {
    return {reinterpret_cast<const uint8_t*>(value.data()),
            static_cast<uint32_t>(value.size())};
  }
  static mp_span span(const std::vector<uint8_t>& value) noexcept {
    return {value.data(), static_cast<uint32_t>(value.size())};
  }
  bool bounded() const noexcept {
    return operation_id_.size() <= MP_MAX_MUTATION_OPERATION_BYTES &&
           operation_id_.size() <= std::numeric_limits<uint32_t>::max() &&
           bytes_.size() <= MP_MAX_MUTATION_CHUNK_BYTES &&
           bytes_.size() <= std::numeric_limits<uint32_t>::max() &&
           content_length_ <= MP_MAX_MUTATION_CONTENT_BYTES;
  }

 public:
  static mutation_request create(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms, uint64_t content_length,
      std::array<uint8_t, 32> content_sha256) {
    mutation_request value(MP_MUTATION_PREPARE_CREATE, call_id, reference,
                           submission, std::move(operation_id), deadline_ms);
    value.content_length_ = content_length;
    value.content_sha256_ = content_sha256;
    return value;
  }
  static mutation_request remove(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms) {
    return mutation_request(MP_MUTATION_PREPARE_DELETE, call_id, reference,
                            submission, std::move(operation_id), deadline_ms);
  }
  static mutation_request chunk(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms, uint64_t offset,
      std::vector<uint8_t> bytes) {
    mutation_request value(MP_MUTATION_CHUNK, call_id, reference, submission,
                           std::move(operation_id), deadline_ms);
    value.offset_ = offset;
    value.bytes_ = std::move(bytes);
    return value;
  }
  static mutation_request commit(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms) {
    return mutation_request(MP_MUTATION_COMMIT, call_id, reference, submission,
                            std::move(operation_id), deadline_ms);
  }
  static mutation_request execute(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms) {
    return mutation_request(MP_MUTATION_EXECUTE, call_id, reference, submission,
                            std::move(operation_id), deadline_ms);
  }
  static mutation_request query(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms) {
    return mutation_request(MP_MUTATION_QUERY, call_id, reference, submission,
                            std::move(operation_id), deadline_ms);
  }
  static mutation_request cancel_plan(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms) {
    return mutation_request(MP_MUTATION_CANCEL_PLAN, call_id, reference,
                            submission, std::move(operation_id), deadline_ms);
  }
  static mutation_request release(uint64_t call_id,
      std::array<uint8_t, 32> reference, std::array<uint8_t, 32> submission,
      std::string operation_id, uint32_t deadline_ms) {
    return mutation_request(MP_MUTATION_RELEASE, call_id, reference, submission,
                            std::move(operation_id), deadline_ms);
  }

  encoded_request encode() const {
    if (!bounded()) return {MP_CODEC_LIMIT, {}};
    mp_mutation_request_v1 raw{};
    raw.abi_version = MP_MUTATION_ABI_VERSION;
    raw.struct_size = sizeof(raw);
    raw.kind = kind_;
    raw.call_id = call_id_;
    raw.reference = span(reference_);
    raw.submission = span(submission_);
    raw.operation_id = span(operation_id_);
    raw.deadline_ms = deadline_ms_;
    if (kind_ == MP_MUTATION_PREPARE_CREATE) {
      raw.content_length = content_length_;
      raw.content_sha256 = span(content_sha256_);
    } else if (kind_ == MP_MUTATION_CHUNK) {
      raw.offset = offset_;
      raw.bytes = span(bytes_);
    }
    encoded_request out{MP_CODEC_OK,
                        std::vector<uint8_t>(MP_MAX_MUTATION_FRAME_BYTES)};
    uint32_t length = 0;
    out.status = mp_mutation_request_encode(&raw, out.bytes.data(),
                                            MP_MAX_MUTATION_FRAME_BYTES, &length);
    out.bytes.resize(out.status == MP_CODEC_OK ? length : 0);
    return out;
  }
};

// Views borrow the owned C response; they expire on destruction or move assignment.
class mutation_response {
  mp_mutation_response* value_ = nullptr;
  uint32_t status_ = MP_CODEC_INVALID;
 public:
  mutation_response() = default;
  ~mutation_response() { mp_mutation_response_free(value_); }
  mutation_response(const mutation_response&) = delete;
  mutation_response& operator=(const mutation_response&) = delete;
  mutation_response(mutation_response&& other) noexcept
      : value_(std::exchange(other.value_, nullptr)),
        status_(std::exchange(other.status_, MP_CODEC_INVALID)) {}
  mutation_response& operator=(mutation_response&& other) noexcept {
    if (this != &other) {
      mp_mutation_response_free(value_);
      value_ = std::exchange(other.value_, nullptr);
      status_ = std::exchange(other.status_, MP_CODEC_INVALID);
    }
    return *this;
  }
  static mutation_response decode(const encoded_request& request,
                                  const std::vector<uint8_t>& response) {
    mutation_response out;
    if (request.status != MP_CODEC_OK) {
      out.status_ = request.status;
      return out;
    }
    if (request.bytes.empty() || response.empty()) {
      out.status_ = MP_CODEC_INVALID;
      return out;
    }
    if (request.bytes.size() > MP_MAX_MUTATION_FRAME_BYTES ||
        response.size() > MP_MAX_MUTATION_FRAME_BYTES ||
        request.bytes.size() > std::numeric_limits<uint32_t>::max() ||
        response.size() > std::numeric_limits<uint32_t>::max()) {
      out.status_ = MP_CODEC_LIMIT;
      return out;
    }
    out.status_ = mp_mutation_response_decode(
        response.data(), static_cast<uint32_t>(response.size()),
        request.bytes.data(), static_cast<uint32_t>(request.bytes.size()),
        &out.value_);
    return out;
  }
#if defined(__wasm32__)
  static mutation_response call_frame(const std::vector<uint8_t>& request_frame) {
    mutation_response out;
    if (request_frame.empty()) {
      out.status_ = MP_CODEC_INVALID;
      return out;
    }
    if (request_frame.size() > MP_MAX_MUTATION_FRAME_BYTES) {
      out.status_ = MP_CODEC_LIMIT;
      return out;
    }
    out.status_ = mp_wasm_mutation_call_frame(
        request_frame.data(), static_cast<uint32_t>(request_frame.size()),
        &out.value_);
    return out;
  }
  static mutation_response call(const mutation_request& request) {
    auto encoded = request.encode();
    if (encoded.status != MP_CODEC_OK) {
      mutation_response out;
      out.status_ = encoded.status;
      return out;
    }
    return call_frame(encoded.bytes);
  }
#endif
  uint32_t status() const noexcept { return status_; }
  mp_mutation_response_view view() const {
    mp_mutation_response_view result{};
    if (status_ != MP_CODEC_OK ||
        mp_mutation_response_get(value_, &result, sizeof(result)) != MP_CODEC_OK)
      detail::codec_logic_error();
    return result;
  }
};
} // namespace morrow

#endif
