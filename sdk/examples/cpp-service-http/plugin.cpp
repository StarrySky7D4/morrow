#include "morrow_plugin_service.hpp"
#include "morrow_plugin_io.hpp"
#include <algorithm>
#include <cstring>
static std::vector<uint8_t> bytes(mp_span v){return v.length?std::vector<uint8_t>(v.data,v.data+v.length):std::vector<uint8_t>{};}
static int32_t answer(const morrow::service_request& request,uint32_t status,mp_span body){
  mp_service_reply_v1 reply{};reply.abi_version=MP_SERVICE_ABI_VERSION;reply.struct_size=sizeof(reply);reply.status=status;reply.body=body;
  return request.complete(reply)==MP_CODEC_OK?0:-1;
}
static int32_t failure(const morrow::service_request& request,uint32_t status,const char *s){return answer(request,status,{reinterpret_cast<const uint8_t*>(s),static_cast<uint32_t>(std::strlen(s))});}
extern "C" int32_t morrow_run(){
  auto request=morrow::service_request::read();if(request.status()!=MP_CODEC_OK)return -1;
  auto v=request.view();if(v.method.length!=4 || std::memcmp(v.method.data,"POST",4))return failure(request,405,"post-required");
  auto resources=request.resources();if(resources.status()!=MP_CODEC_OK)return -1;
  if(!resources.present())return failure(request,503,"one-endpoint-required");
  auto directory=resources.view();if(directory.endpoint_count!=1)return failure(request,503,"one-endpoint-required");
  const auto& endpoint=directory.endpoints[0];
  bool allowed=false;for(uint32_t i=0;i<endpoint.method_count;i++)if(endpoint.methods[i].length==4 && !std::memcmp(endpoint.methods[i].data,"POST",4))allowed=true;
  if(!allowed)return failure(request,403,"method-denied");
  if(v.body.length>endpoint.max_request_bytes)return failure(request,413,"request-too-large");
  std::string operation="service-http-";const char *hex="0123456789abcdef";
  for(auto b:request.digest()){operation+=hex[b>>4];operation+=hex[b&15];}
  auto outbound=morrow::io_request::http_request(v.call_id,{operation.begin(),operation.end()},bytes(endpoint.reference),
      "POST","/",{},bytes(v.body),bytes(endpoint.credential),std::min<uint64_t>(endpoint.timeout_ms,30000));
  auto response=morrow::io_response::call(outbound);if(response.status()!=MP_CODEC_OK)return -1;
  auto result=response.view();
  switch(result.status){
    case MP_IO_STATUS_COMPLETED:return answer(request,result.http_status,result.bytes);
    case MP_IO_STATUS_OUTCOME_UNKNOWN:return failure(request,409,"outcome-unknown");
    case MP_IO_STATUS_CONFLICT:return failure(request,409,"operation-conflict");
    case MP_IO_STATUS_DENIED:case MP_IO_STATUS_REVOKED:return failure(request,403,"outbound-denied");
    case MP_IO_STATUS_EXPIRED:return failure(request,504,"outbound-expired");
    case MP_IO_STATUS_QUOTA:return failure(request,429,"outbound-quota");
    default:return failure(request,502,"outbound-unavailable");
  }
}
