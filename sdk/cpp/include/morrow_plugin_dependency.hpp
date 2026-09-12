#ifndef MORROW_PLUGIN_DEPENDENCY_HPP
#define MORROW_PLUGIN_DEPENDENCY_HPP
#include "morrow_plugin_dependency.h"
#include "morrow_plugin_codec.hpp"
namespace morrow {
class dependency_output;
class dependency_request {
  std::string call_id_,slot_; std::vector<uint8_t> input_;
  mp_dependency_request_v1 descriptor() const {
    if(call_id_.size()>256 || slot_.size()>256 || input_.size()>MP_MAX_DEPENDENCY_VALUE_BYTES)
      detail::codec_length_error();
    return {1,sizeof(mp_dependency_request_v1),
      {reinterpret_cast<const uint8_t*>(call_id_.data()),static_cast<uint32_t>(call_id_.size())},
      {reinterpret_cast<const uint8_t*>(slot_.data()),static_cast<uint32_t>(slot_.size())},
      {input_.data(),static_cast<uint32_t>(input_.size())}};
  }
  bool bounded() const {return call_id_.size()<=256 && slot_.size()<=256 && input_.size()<=MP_MAX_DEPENDENCY_VALUE_BYTES;}
  friend class dependency_output;
public:
  dependency_request(std::string call_id,std::string slot,std::vector<uint8_t> input)
    :call_id_(std::move(call_id)),slot_(std::move(slot)),input_(std::move(input)){}
  encoded_request encode() const {
    if(!bounded())return {MP_CODEC_LIMIT,{}};
    auto request=descriptor(); encoded_request out{MP_CODEC_OK,std::vector<uint8_t>(MP_MAX_DEPENDENCY_BYTES)};
    uint32_t length=0;out.status=mp_dependency_request_encode(&request,out.bytes.data(),MP_MAX_DEPENDENCY_BYTES,&length);
    out.bytes.resize(out.status==MP_CODEC_OK?length:0);return out;
  }
};
inline encoded_request encode_dependency(const dependency_request& request){return request.encode();}
class dependency_output {
  mp_dependency_output* value_=nullptr; uint32_t status_=MP_CODEC_INVALID;
public:
  dependency_output()=default;
  ~dependency_output(){mp_dependency_output_free(value_);}
  dependency_output(const dependency_output&)=delete;
  dependency_output& operator=(const dependency_output&)=delete;
  dependency_output(dependency_output&& other) noexcept
    :value_(std::exchange(other.value_,nullptr)),status_(std::exchange(other.status_,MP_CODEC_INVALID)){}
  dependency_output& operator=(dependency_output&& other) noexcept {
    if(this!=&other){mp_dependency_output_free(value_);value_=std::exchange(other.value_,nullptr);status_=std::exchange(other.status_,MP_CODEC_INVALID);}return *this;
  }
  static dependency_output decode(const encoded_request& request,const std::vector<uint8_t>& response){
    dependency_output out;
    if(request.status!=MP_CODEC_OK){out.status_=request.status;return out;}
    if(request.bytes.size()>MP_MAX_DEPENDENCY_BYTES || response.size()>MP_MAX_DEPENDENCY_BYTES){out.status_=MP_CODEC_LIMIT;return out;}
    out.status_=mp_dependency_response_decode(response.data(),static_cast<uint32_t>(response.size()),request.bytes.data(),static_cast<uint32_t>(request.bytes.size()),&out.value_);return out;
  }
#if defined(__wasm32__)
  static dependency_output call(const dependency_request& request){
    dependency_output out;if(!request.bounded()){out.status_=MP_CODEC_LIMIT;return out;}
    auto raw=request.descriptor();out.status_=mp_wasm_dependency_call(&raw,&out.value_);return out;
  }
#endif
  uint32_t status() const{return status_;}
  mp_dependency_output_view view() const{
    mp_dependency_output_view result{};
    if(status_!=MP_CODEC_OK || mp_dependency_output_get(value_,&result,sizeof(result))!=MP_CODEC_OK)detail::codec_logic_error();
    return result;
  }
};
}
#endif
