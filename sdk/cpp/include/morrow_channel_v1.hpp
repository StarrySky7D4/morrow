#ifndef MORROW_CHANNEL_V1_HPP
#define MORROW_CHANNEL_V1_HPP
#include "morrow_channel_v1.h"
#include <array>
#include <algorithm>
#include <vector>
#include <utility>
namespace morrow { namespace channel_v1 {
struct request {
  uint32_t kind=MP_CHANNEL_QUERY,credit_bytes=0;
  uint64_t sequence=0;
  std::array<uint8_t,32> call_id{},reference{},source_epoch{},frame_sha256{};
  std::vector<uint8_t> cursor,bytes;
  mp_channel_request_v1 view() const noexcept {
    mp_channel_request_v1 r{};r.abi_version=1;r.struct_size=sizeof(r);r.kind=kind;
    r.call_id={call_id.data(),32};r.reference={reference.data(),32};r.source_epoch={source_epoch.data(),32};
    r.sequence=sequence;r.credit_bytes=credit_bytes;r.frame_sha256={frame_sha256.data(),32};
    r.cursor={cursor.data(),static_cast<uint32_t>(cursor.size())};r.bytes={bytes.data(),static_cast<uint32_t>(bytes.size())};return r;
  }
  bool bounded() const noexcept {return bytes.size()<=MP_MAX_CHANNEL_PAYLOAD_BYTES && cursor.size()<=MP_MAX_CHANNEL_CURSOR_BYTES;}
};
class response {
  mp_channel_response *handle_=nullptr;
  uint32_t code_=MP_CODEC_INVALID;
public:
  response()=default;
  ~response(){mp_channel_response_free(handle_);}
  response(const response&)=delete;response& operator=(const response&)=delete;
  response(response&& r) noexcept:handle_(std::exchange(r.handle_,nullptr)),code_(r.code_){}
  response& operator=(response&& r) noexcept {if(this!=&r){mp_channel_response_free(handle_);handle_=std::exchange(r.handle_,nullptr);code_=r.code_;}return *this;}
  static response call(const mp_channel_host_v1& host,const request& r) noexcept {
    response out;if(!r.bounded()){out.code_=MP_CODEC_LIMIT;return out;}auto raw=r.view();
    out.code_=mp_channel_call(&host,&raw,&out.handle_);return out;
  }
#if defined(__wasm32__)
  static response call(const request& r) noexcept {
    response out;if(!r.bounded()){out.code_=MP_CODEC_LIMIT;return out;}auto raw=r.view();
    out.code_=mp_wasm_channel_call(&raw,&out.handle_);return out;
  }
#endif
  uint32_t code() const noexcept {return code_;}
  uint32_t view(mp_channel_response_view& out) const noexcept {
    out={};return code_==MP_CODEC_OK?mp_channel_response_get(handle_,&out,sizeof(out)):code_;
  }
};
class directory {
  mp_channel_directory *handle_=nullptr;
  uint32_t code_=MP_CODEC_INVALID;
public:
  directory()=default;
  ~directory(){mp_channel_directory_free(handle_);}
  directory(const directory&)=delete;directory& operator=(const directory&)=delete;
  directory(directory&& d) noexcept:handle_(std::exchange(d.handle_,nullptr)),code_(d.code_){}
  directory& operator=(directory&& d) noexcept {if(this!=&d){mp_channel_directory_free(handle_);handle_=std::exchange(d.handle_,nullptr);code_=d.code_;}return *this;}
  static directory decode(const std::vector<uint8_t>& bytes) noexcept {
    directory out;if(bytes.empty()){out.code_=MP_CODEC_INVALID;return out;}if(bytes.size()>MP_MAX_CHANNEL_WIRE_BYTES){out.code_=MP_CODEC_LIMIT;return out;}
    out.code_=mp_channel_directory_decode(bytes.data(),static_cast<uint32_t>(bytes.size()),&out.handle_);return out;
  }
  uint32_t code() const noexcept {return code_;}
  uint32_t view(mp_channel_directory_view& out) const noexcept {out={};return code_==MP_CODEC_OK?mp_channel_directory_get(handle_,&out,sizeof(out)):code_;}
};
/* Caller owns callable for the host's lifetime. Every C++ exception is translated
 * to uncertain transport failure inside this noexcept C callback boundary. */
template<class Callable> struct local_adapter {
  static uint32_t invoke(void *ctx,const uint8_t *input,uint32_t length,uint8_t *output,uint32_t capacity,uint32_t *written) noexcept {
    if(!ctx||!written)return MP_INVALID_ARGUMENT;
    *written=0;
#if defined(__cpp_exceptions)
    try {return (*static_cast<Callable*>(ctx))(input,length,output,capacity,written);}
    catch(...) {*written=0;return MP_TRANSPORT_FAILURE;}
#else
    return (*static_cast<Callable*>(ctx))(input,length,output,capacity,written);
#endif
  }
  static mp_channel_host_v1 host(Callable& callable) noexcept {return {1,sizeof(mp_channel_host_v1),&callable,invoke};}
};
} }
#endif
