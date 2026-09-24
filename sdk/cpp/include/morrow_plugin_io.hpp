#ifndef MORROW_PLUGIN_IO_HPP
#define MORROW_PLUGIN_IO_HPP
#include "morrow_plugin_io.h"
#include "morrow_plugin_codec.hpp"
#include <array>
namespace morrow {
struct io_header {
  std::string name;
  std::vector<uint8_t> value;
};
class io_request {
  uint32_t kind_;
  uint64_t call_id_,deadline_ms_=0,offset_=0;
  uint32_t limit_=0;
  std::vector<uint8_t> reference_,operation_id_,endpoint_,credential_,body_;
  std::string method_,relative_target_;
  std::vector<io_header> headers_;
  io_request(uint32_t kind,uint64_t call_id):kind_(kind),call_id_(call_id){}
  static mp_span span(const std::vector<uint8_t>& v){return {v.data(),static_cast<uint32_t>(v.size())};}
  static mp_span span(const std::string& v){return {reinterpret_cast<const uint8_t*>(v.data()),static_cast<uint32_t>(v.size())};}
  bool bounded() const {
    if (reference_.size()>32 || operation_id_.size()>256 || endpoint_.size()>256 || credential_.size()>4096 ||
        body_.size()>MP_MAX_IO_PAYLOAD_BYTES || method_.size()>16 || relative_target_.size()>2048 || headers_.size()>MP_MAX_IO_HEADERS)
      return false;
    size_t total=0;
    for(const auto& h:headers_){if(h.name.size()>128 || h.value.size()>8192)return false;total+=h.name.size()+h.value.size();}
    return total<=16384;
  }
public:
  static io_request file_read(uint64_t call_id,std::vector<uint8_t> operation_id,std::array<uint8_t,32> reference,uint64_t deadline_ms){
    io_request v(MP_IO_FILE_READ,call_id);v.operation_id_=std::move(operation_id);v.reference_.assign(reference.begin(),reference.end());v.deadline_ms_=deadline_ms;return v;
  }
  static io_request http_request(uint64_t call_id,std::vector<uint8_t> operation_id,std::vector<uint8_t> endpoint,
      std::string method,std::string relative_target,std::vector<io_header> headers,std::vector<uint8_t> body,
      std::vector<uint8_t> credential,uint64_t deadline_ms){
    io_request v(MP_IO_HTTP_REQUEST,call_id);v.operation_id_=std::move(operation_id);v.endpoint_=std::move(endpoint);
    v.method_=std::move(method);v.relative_target_=std::move(relative_target);v.headers_=std::move(headers);
    v.body_=std::move(body);v.credential_=std::move(credential);v.deadline_ms_=deadline_ms;return v;
  }
  static io_request poll(uint64_t call_id,std::array<uint8_t,32> reference){io_request v(MP_IO_POLL,call_id);v.reference_.assign(reference.begin(),reference.end());return v;}
  static io_request read(uint64_t call_id,std::array<uint8_t,32> reference,uint64_t offset,uint32_t limit){io_request v=poll(call_id,reference);v.kind_=MP_IO_READ;v.offset_=offset;v.limit_=limit;return v;}
  static io_request finish(uint64_t call_id,std::array<uint8_t,32> reference){io_request v=poll(call_id,reference);v.kind_=MP_IO_FINISH;return v;}
  static io_request cancel(uint64_t call_id,std::array<uint8_t,32> reference){io_request v=poll(call_id,reference);v.kind_=MP_IO_CANCEL;return v;}
  static io_request query_operation(uint64_t call_id,std::vector<uint8_t> operation_id){io_request v(MP_IO_QUERY_OPERATION,call_id);v.operation_id_=std::move(operation_id);return v;}
  encoded_request encode() const {
    if(!bounded())return {MP_CODEC_LIMIT,{}};
    std::vector<mp_io_header> headers;headers.reserve(headers_.size());
    for(const auto& h:headers_)headers.push_back({span(h.name),span(h.value)});
    mp_io_request_v1 raw{};raw.abi_version=MP_IO_ABI_VERSION;raw.struct_size=sizeof(raw);raw.kind=kind_;raw.call_id=call_id_;
    raw.reference=span(reference_);raw.operation_id=span(operation_id_);raw.endpoint=span(endpoint_);
    raw.method=span(method_);raw.relative_target=span(relative_target_);raw.credential=span(credential_);raw.body=span(body_);
    raw.headers=headers.data();raw.header_count=static_cast<uint32_t>(headers.size());raw.deadline_ms=deadline_ms_;raw.offset=offset_;raw.limit=limit_;
    encoded_request out{MP_CODEC_OK,std::vector<uint8_t>(MP_MAX_IO_FRAME_BYTES)};uint32_t length=0;
    out.status=mp_io_request_encode(&raw,out.bytes.data(),MP_MAX_IO_FRAME_BYTES,&length);
    out.bytes.resize(out.status==MP_CODEC_OK?length:0);return out;
  }
};
class io_response {
  mp_io_response* value_=nullptr;
  uint32_t status_=MP_CODEC_INVALID;
public:
  io_response()=default;
  ~io_response(){mp_io_response_free(value_);}
  io_response(const io_response&)=delete;io_response& operator=(const io_response&)=delete;
  io_response(io_response&& other) noexcept:value_(std::exchange(other.value_,nullptr)),status_(std::exchange(other.status_,MP_CODEC_INVALID)){}
  io_response& operator=(io_response&& other) noexcept {if(this!=&other){mp_io_response_free(value_);value_=std::exchange(other.value_,nullptr);status_=std::exchange(other.status_,MP_CODEC_INVALID);}return *this;}
  static io_response decode(const encoded_request& request,const std::vector<uint8_t>& response){
    io_response out;if(request.status!=MP_CODEC_OK){out.status_=request.status;return out;}
    if(request.bytes.size()>MP_MAX_IO_FRAME_BYTES || response.size()>MP_MAX_IO_FRAME_BYTES){out.status_=MP_CODEC_LIMIT;return out;}
    out.status_=mp_io_response_decode(response.data(),static_cast<uint32_t>(response.size()),request.bytes.data(),static_cast<uint32_t>(request.bytes.size()),&out.value_);return out;
  }
#if defined(__wasm32__)
  static io_response call_frame(const std::vector<uint8_t>& request_frame){
    io_response out;
    if(request_frame.empty()){out.status_=MP_CODEC_INVALID;return out;}
    if(request_frame.size()>MP_MAX_IO_FRAME_BYTES){out.status_=MP_CODEC_LIMIT;return out;}
    out.status_=mp_wasm_io_call_frame(request_frame.data(),static_cast<uint32_t>(request_frame.size()),&out.value_);
    return out;
  }
  static io_response call(const io_request& request){
    io_response out;auto encoded=request.encode();if(encoded.status!=MP_CODEC_OK){out.status_=encoded.status;return out;}
    return call_frame(encoded.bytes);
  }
#endif
  uint32_t status() const{return status_;}
  mp_io_response_view view() const {mp_io_response_view v{};if(status_!=MP_CODEC_OK || mp_io_response_get(value_,&v,sizeof(v))!=MP_CODEC_OK)detail::codec_logic_error();return v;}
};
} // namespace morrow
#endif
