#include "morrow_channel_v1.h"
#include "morrow_plugin_task.h"
#include <stdlib.h>
#include <string.h>
static void put32(uint8_t *p,uint32_t n){for(uint32_t i=0;i<4;i++)p[i]=(uint8_t)(n>>(8*i));}
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=(uint8_t)(n>>(8*i));}
static int equals(mp_span value,const char *text){size_t n=strlen(text);return value.length==n&&memcmp(value.data,text,n)==0;}
static uint32_t submit(mp_channel_request_v1 *request,const mp_channel_directory_view *directory,uint64_t *calls,mp_channel_response **response){
  static const uint8_t domain[]="morrow.channel.directory.request.v1";
  mp_channel_digest *id=NULL;uint8_t counter[8],call_id[32];uint32_t status=mp_channel_digest_new(&id);
  put64(counter,++*calls);
  if(status==MP_CODEC_OK)status=mp_channel_digest_update(id,domain,(uint32_t)(sizeof(domain)-1));
  if(status==MP_CODEC_OK)status=mp_channel_digest_update(id,directory->scope_sha256.data,directory->scope_sha256.length);
  if(status==MP_CODEC_OK)status=mp_channel_digest_update(id,request->reference.data,request->reference.length);
  if(status==MP_CODEC_OK)status=mp_channel_digest_update(id,request->source_epoch.data,request->source_epoch.length);
  if(status==MP_CODEC_OK)status=mp_channel_digest_update(id,counter,8);
  if(status==MP_CODEC_OK)status=mp_channel_digest_finish(id,call_id,32);
  if(status==MP_CODEC_OK){request->call_id.data=call_id;request->call_id.length=32;status=mp_wasm_channel_call(request,response);}
  request->call_id.data=NULL;request->call_id.length=0;mp_channel_digest_free(id);return status;
}
int32_t morrow_run(void){
  uint8_t *input=NULL,*completion=NULL;mp_task *task=NULL;mp_channel_directory *directory=NULL;mp_channel_digest *digest=NULL;
  mp_channel_response *response=NULL,*ack=NULL;mp_transform_view t={0};mp_channel_directory_view d={0};mp_channel_response_view v={0},av={0};
  mp_channel_request_v1 request={0};uint8_t summary[64]={0};uint32_t status=MP_CHANNEL_READY,reclaimed=0,mode=0,length=0;
  uint64_t calls=0,count=0,total=0,last_acked=0,maximum=0;int32_t result=-1;
  input=(uint8_t*)malloc(MP_MAX_TASK_BYTES);completion=(uint8_t*)malloc(MP_MAX_TASK_BYTES);if(!input||!completion)goto done;
  int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);
  if(n<=0||(uint32_t)n>MP_MAX_TASK_BYTES||mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
  if(mp_task_get_transform(task,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"channel.directory.consume")||!equals(t.input_type,"morrow.channel.directory.v1")||!equals(t.output_type,"bytes"))goto done;
  if(mp_channel_directory_decode(t.input.data,t.input.length,&directory)!=MP_CODEC_OK||mp_channel_directory_get(directory,&d,sizeof(d))!=MP_CODEC_OK||d.channel_count!=1)goto done;
  const mp_channel_endpoint_view *endpoint=&d.channels[0];if(endpoint->kind>1)goto done;mode=endpoint->kind==0?0:2;
  request.abi_version=1;request.struct_size=sizeof(request);request.reference=endpoint->reference;request.source_epoch=endpoint->source_epoch;
  maximum=endpoint->budget.max_messages;if(maximum>32)maximum=32;if(maximum>(endpoint->budget.max_requests-1)/2)maximum=(endpoint->budget.max_requests-1)/2;
  if(mp_channel_digest_new(&digest)!=MP_CODEC_OK)goto done;
  for(uint64_t i=0;i<maximum;i++){
    request.kind=MP_CHANNEL_RECEIVE;request.sequence=last_acked;request.credit_bytes=endpoint->budget.max_frame_bytes;
    if(submit(&request,&d,&calls,&response)!=MP_CODEC_OK||mp_channel_response_get(response,&v,sizeof(v))!=MP_CODEC_OK)goto done;
    status=v.status;reclaimed=v.resource_reclaimed;if(status!=MP_CHANNEL_FRAME){mp_channel_response_free(response);response=NULL;break;}
    if(!v.has_frame||mp_channel_digest_update(digest,v.bytes.data,v.bytes.length)!=MP_CODEC_OK)goto done;count++;total+=v.bytes.length;
    request.kind=MP_CHANNEL_ACK;request.sequence=v.sequence;request.frame_sha256=v.frame_sha256;request.cursor=v.cursor;
    if(submit(&request,&d,&calls,&ack)!=MP_CODEC_OK||mp_channel_response_get(ack,&av,sizeof(av))!=MP_CODEC_OK)goto done;
    status=av.status;reclaimed=av.resource_reclaimed;last_acked=v.sequence;
    mp_channel_response_free(ack);ack=NULL;mp_channel_response_free(response);response=NULL;
    request.frame_sha256.data=NULL;request.frame_sha256.length=0;request.cursor.data=NULL;request.cursor.length=0;
    if(status!=MP_CHANNEL_ACKED)break;
  }
  if(status==MP_CHANNEL_READY||status==MP_CHANNEL_IDLE||status==MP_CHANNEL_ACKED){
    request.kind=MP_CHANNEL_CLOSE;
    if(submit(&request,&d,&calls,&response)!=MP_CODEC_OK||mp_channel_response_get(response,&v,sizeof(v))!=MP_CODEC_OK)goto done;
    status=v.status;reclaimed=v.resource_reclaimed;
  }
  memcpy(summary,"CHV1",4);put32(summary+4,mode);put32(summary+8,status);put32(summary+12,reclaimed);put64(summary+16,count);put64(summary+24,total);
  if(mp_channel_digest_finish(digest,summary+32,32)!=MP_CODEC_OK||mp_task_output(task,summary,64,completion,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
  result=mp_wasm_task_complete(completion,length);
done:
  mp_channel_response_free(ack);mp_channel_response_free(response);mp_channel_digest_free(digest);mp_channel_directory_free(directory);mp_task_free(task);free(completion);free(input);return result;
}
