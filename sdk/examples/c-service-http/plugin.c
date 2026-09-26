/* One host-selected POST endpoint; no caller URL/credentials or automatic retry. */
#include "morrow_plugin_service.h"
#include "morrow_plugin_io.h"
#include <string.h>
static mp_span text(const char *s){mp_span v={(const uint8_t*)s,(uint32_t)strlen(s)};return v;}
static int32_t answer(mp_service_request *request,uint32_t status,mp_span body){
  mp_service_reply_v1 reply={0};reply.abi_version=MP_SERVICE_ABI_VERSION;reply.struct_size=sizeof(reply);
  reply.status=status;reply.body=body;
  return mp_wasm_service_complete(request,&reply)==MP_CODEC_OK?0:-1;
}
int32_t morrow_run(void){
  mp_service_request *request=NULL;mp_service_resources *resources=NULL;mp_io_response *response=NULL;
  mp_service_request_view v={0};mp_service_resources_view r={0};mp_io_response_view result={0};
  mp_io_request_v1 outbound={0};uint8_t digest[32],operation[77];int32_t code=-1;uint32_t status=502;
  mp_span body=text("outbound-unavailable");
  const char *hex="0123456789abcdef";
  if(mp_wasm_service_read(&request)!=MP_CODEC_OK)goto done;
  if(mp_service_request_get(request,&v,sizeof(v))!=MP_CODEC_OK)goto done;
  if(v.method.length!=4 || memcmp(v.method.data,"POST",4)!=0){code=answer(request,405,text("post-required"));goto done;}
  if(mp_service_request_resources(request,&resources)!=MP_CODEC_OK)goto done;
  if(!resources){code=answer(request,503,text("one-endpoint-required"));goto done;}
  if(mp_service_resources_get(resources,&r,sizeof(r))!=MP_CODEC_OK)goto done;
  if(r.endpoint_count!=1){code=answer(request,503,text("one-endpoint-required"));goto done;}
  const mp_service_endpoint *endpoint=&r.endpoints[0];
  uint32_t allowed=0;
  for(uint32_t i=0;i<endpoint->method_count;i++)if(endpoint->methods[i].length==4 && memcmp(endpoint->methods[i].data,"POST",4)==0)allowed=1;
  if(!allowed){code=answer(request,403,text("method-denied"));goto done;}
  if(v.body.length>endpoint->max_request_bytes){code=answer(request,413,text("request-too-large"));goto done;}
  if(mp_service_request_digest(request,digest,sizeof(digest))!=MP_CODEC_OK)goto done;
  memcpy(operation,"service-http-",13);
  for(uint32_t i=0;i<32;i++){operation[13+i*2]=(uint8_t)hex[digest[i]>>4];operation[14+i*2]=(uint8_t)hex[digest[i]&15];}
  outbound.abi_version=MP_IO_ABI_VERSION;outbound.struct_size=sizeof(outbound);outbound.kind=MP_IO_HTTP_REQUEST;
  outbound.call_id=v.call_id;outbound.operation_id=(mp_span){operation,sizeof(operation)};
  outbound.endpoint=endpoint->reference;outbound.credential=endpoint->credential;
  outbound.method=text("POST");outbound.relative_target=text("/");outbound.body=v.body;
  outbound.deadline_ms=endpoint->timeout_ms<30000?endpoint->timeout_ms:30000;
  if(mp_wasm_io_call(&outbound,&response)!=MP_CODEC_OK)goto done;
  if(mp_io_response_get(response,&result,sizeof(result))!=MP_CODEC_OK)goto done;
  switch(result.status){
    case MP_IO_STATUS_COMPLETED:status=result.http_status;body=result.bytes;break;
    case MP_IO_STATUS_OUTCOME_UNKNOWN:status=409;body=text("outcome-unknown");break;
    case MP_IO_STATUS_CONFLICT:status=409;body=text("operation-conflict");break;
    case MP_IO_STATUS_DENIED:case MP_IO_STATUS_REVOKED:status=403;body=text("outbound-denied");break;
    case MP_IO_STATUS_EXPIRED:status=504;body=text("outbound-expired");break;
    case MP_IO_STATUS_QUOTA:status=429;body=text("outbound-quota");break;
  }
  code=answer(request,status,body);
done:
  mp_io_response_free(response);mp_service_resources_free(resources);mp_service_request_free(request);return code;
}
