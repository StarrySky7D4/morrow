#ifndef MORROW_FS_DIRECTORY_REQUEST_V1_HPP
#define MORROW_FS_DIRECTORY_REQUEST_V1_HPP
#include "morrow_fs_directory_request_v1.h"
#include <utility>
#include <vector>
namespace morrow::fs_directory_request_v1 {
class request {
 mdr_request *handle_=nullptr;
public:
 request()=default;~request(){mdr_request_free(handle_);}
 request(const request&)=delete;request& operator=(const request&)=delete;
 request(request&& other) noexcept:handle_(std::exchange(other.handle_,nullptr)){}
 request& operator=(request&& other) noexcept {if(this!=&other){mdr_request_free(handle_);handle_=std::exchange(other.handle_,nullptr);}return *this;}
 const mdr_request* native() const noexcept{return handle_;}
 void adopt(mdr_request *value) noexcept {mdr_request_free(handle_);handle_=value;}
 uint32_t assign(const mdr_request_view& v){mdr_request *value=nullptr;auto s=mdr_request_set(&v,sizeof(v),&value);if(s==MDR_OK)adopt(value);return s;}
 uint32_t decode(const uint8_t *bytes,uint32_t length){mdr_request *value=nullptr;auto s=mdr_request_decode(bytes,length,&value);if(s==MDR_OK)adopt(value);return s;}
 uint32_t view(mdr_request_view& v) const{return mdr_request_get_view(handle_,&v,sizeof(v));}
 uint32_t encode(std::vector<uint8_t>& output) const {std::vector<uint8_t> temp(MDR_MAX_REQUEST_BYTES);uint32_t length=0;auto s=mdr_request_encode(handle_,temp.data(),static_cast<uint32_t>(temp.size()),&length);if(s==MDR_OK){temp.resize(length);output.swap(temp);}return s;}
};
class response {
 mdr_response *handle_=nullptr;
public:
 response()=default;~response(){mdr_response_free(handle_);}
 response(const response&)=delete;response& operator=(const response&)=delete;
 response(response&& other) noexcept:handle_(std::exchange(other.handle_,nullptr)){}
 response& operator=(response&& other) noexcept {if(this!=&other){mdr_response_free(handle_);handle_=std::exchange(other.handle_,nullptr);}return *this;}
 const mdr_response* native() const noexcept{return handle_;}
 void adopt(mdr_response *value) noexcept {mdr_response_free(handle_);handle_=value;}
 uint32_t assign(const request& q,const mdr_reply_view& v){mdr_response *value=nullptr;auto s=mdr_response_set(q.native(),&v,sizeof(v),&value);if(s==MDR_OK)adopt(value);return s;}
 uint32_t decode_for(const uint8_t *bytes,uint32_t length,const request& q){mdr_response *value=nullptr;auto s=mdr_response_decode_for(bytes,length,q.native(),&value);if(s==MDR_OK)adopt(value);return s;}
 /* page_wire in the view remains owned by this response until replacement/drop. */
 uint32_t view(mdr_response_view& v) const{return mdr_response_get_view(handle_,&v,sizeof(v));}
 uint32_t encode(std::vector<uint8_t>& output) const {std::vector<uint8_t> temp(MDR_MAX_RESPONSE_BYTES);uint32_t length=0;auto s=mdr_response_encode(handle_,temp.data(),static_cast<uint32_t>(temp.size()),&length);if(s==MDR_OK){temp.resize(length);output.swap(temp);}return s;}
};
class client {
 mdr_client *handle_=nullptr;
public:
 client()=default;~client(){mdr_client_free(handle_);}
 client(const client&)=delete;client& operator=(const client&)=delete;
 client(client&& other) noexcept:handle_(std::exchange(other.handle_,nullptr)){}
 client& operator=(client&& other) noexcept {if(this!=&other){mdr_client_free(handle_);handle_=std::exchange(other.handle_,nullptr);}return *this;}
 uint32_t initialize(const uint8_t reference[32]){mdr_client *value=nullptr;auto s=mdr_client_new(reference,32,&value);if(s==MDR_OK){mdr_client_free(handle_);handle_=value;}return s;}
 uint32_t snapshot(mdr_snapshot& value) const{return mdr_client_snapshot(handle_,&value,sizeof(value));}
 uint32_t begin(const mdr_request_view& v,request& output){mdr_request *value=nullptr;auto s=mdr_client_begin(handle_,&v,sizeof(v),&value);if(s==MDR_OK)output.adopt(value);return s;}
 uint32_t begin(const request& q,request& output){mdr_request *value=nullptr;auto s=mdr_client_begin_request(handle_,q.native(),&value);if(s==MDR_OK)output.adopt(value);return s;}
 uint32_t accept(const uint8_t *bytes,uint32_t length,response& output){mdr_response *value=nullptr;auto s=mdr_client_accept(handle_,bytes,length,&value);if(s==MDR_OK)output.adopt(value);return s;}
 uint32_t transport_unknown(){return mdr_client_transport_unknown(handle_);}
 uint32_t call_once(const mdr_request_view& v,response& output,mdr_transport_fn transport=nullptr,void *context=nullptr){mdr_response *value=nullptr;auto s=mdr_client_call_once(handle_,&v,sizeof(v),transport,context,&value);if(s==MDR_OK)output.adopt(value);return s;}
 uint32_t call_once(const request& q,response& output,mdr_transport_fn transport=nullptr,void *context=nullptr){mdr_response *value=nullptr;auto s=mdr_client_call_request_once(handle_,q.native(),transport,context,&value);if(s==MDR_OK)output.adopt(value);return s;}
};
}
#endif
