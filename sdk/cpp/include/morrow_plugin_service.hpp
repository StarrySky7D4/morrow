#ifndef MORROW_PLUGIN_SERVICE_HPP
#define MORROW_PLUGIN_SERVICE_HPP
#include "morrow_plugin_service.h"
#include "morrow_plugin_codec.hpp"
namespace morrow {
class service_request {
  mp_service_request *value_=nullptr;
  uint32_t status_=MP_CODEC_INVALID;
public:
  service_request()=default;
  ~service_request(){mp_service_request_free(value_);}
  service_request(const service_request&)=delete;
  service_request& operator=(const service_request&)=delete;
  service_request(service_request&& other) noexcept
      :value_(std::exchange(other.value_,nullptr)),status_(std::exchange(other.status_,MP_CODEC_INVALID)){}
  service_request& operator=(service_request&& other) noexcept {
    if(this!=&other){mp_service_request_free(value_);value_=std::exchange(other.value_,nullptr);status_=std::exchange(other.status_,MP_CODEC_INVALID);}
    return *this;
  }
  static service_request decode(const std::vector<uint8_t>& bytes){
    service_request out;
    if(bytes.size()>MP_MAX_SERVICE_FRAME_BYTES){out.status_=MP_CODEC_LIMIT;return out;}
    out.status_=mp_service_request_decode(bytes.data(),static_cast<uint32_t>(bytes.size()),&out.value_);
    return out;
  }
  uint32_t status() const{return status_;}
  mp_service_request_view view() const {
    mp_service_request_view v{};
    if(status_!=MP_CODEC_OK || mp_service_request_get(value_,&v,sizeof(v))!=MP_CODEC_OK)detail::codec_logic_error();
    return v;
  }
  // Descriptor spans only borrow during this call. Returned frame owns bytes.
  encoded_request response(const mp_service_reply_v1& reply) const {
    if(status_!=MP_CODEC_OK)return {status_,{}};
    encoded_request out{MP_CODEC_OK,std::vector<uint8_t>(MP_MAX_SERVICE_FRAME_BYTES)};
    uint32_t length=0;
    out.status=mp_service_response_encode(value_,&reply,out.bytes.data(),MP_MAX_SERVICE_FRAME_BYTES,&length);
    out.bytes.resize(out.status==MP_CODEC_OK?length:0);return out;
  }
#if defined(__wasm32__)
  static service_request read(){service_request out;out.status_=mp_wasm_service_read(&out.value_);return out;}
  uint32_t complete(const mp_service_reply_v1& reply) const {
    return status_==MP_CODEC_OK?mp_wasm_service_complete(value_,&reply):status_;
  }
#endif
};
} // namespace morrow
#endif
