/* Original channel transport plus independent strict metadata codec. */
#include "morrow_channel_v1.h"
#include "morrow_plugin_task.h"
#include "morrow_changes_metadata_v1.h"
#include <stdlib.h>
#include <string.h>
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=(uint8_t)(n>>(8*i));}
static int equals(mp_span v,const char *s){size_t n=strlen(s);return v.length==n&&memcmp(v.data,s,n)==0;}
static uint32_t submit(mp_channel_request_v1 *r,uint64_t *calls,uint64_t max_calls,uint8_t call_id[32],mp_channel_response **reply){
  if(*calls>=max_calls)return MP_CODEC_LIMIT;
  memset(call_id,0x47,32);put64(call_id,++*calls);r->call_id.data=call_id;r->call_id.length=32;
  return mp_wasm_channel_call(r,reply);
}
int32_t morrow_run(void){
  uint8_t *input=NULL,*completion=NULL;mp_task *task=NULL;mp_channel_response *reply=NULL,*ack=NULL;
  mp_channel_directory *directory=NULL;mp_channel_directory_view d={0};
  mp_transform_view t;mp_channel_response_view v={0},a={0};mc_metadata_v1 metadata;
  uint8_t call_id[32],summary[8],scope[32];uint32_t length=0;uint64_t calls=0,total=0,count=0;int32_t result=-1;
  mp_channel_request_v1 r={0};r.abi_version=1;r.struct_size=sizeof(r);
  input=(uint8_t*)malloc(MP_MAX_TASK_BYTES);completion=(uint8_t*)malloc(MP_MAX_TASK_BYTES);
  if(!input||!completion)goto done;
  int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);
  if(n<=0||(uint32_t)n>MP_MAX_TASK_BYTES||mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
  if(mp_task_get_transform(task,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"changes.metadata")||!equals(t.input_type,"morrow.channel.directory.v1")||!equals(t.output_type,"morrow.changes.metadata.count.v1"))goto done;
  if(mp_channel_directory_decode(t.input.data,t.input.length,&directory)!=MP_CODEC_OK||mp_channel_directory_get(directory,&d,sizeof(d))!=MP_CODEC_OK||d.channel_count!=1||d.channels[0].kind!=1)goto done;
  const mp_channel_endpoint_view *endpoint=&d.channels[0];
  r.reference=endpoint->reference;r.source_epoch=endpoint->source_epoch;
  for(;;){
    r.kind=MP_CHANNEL_RECEIVE;r.sequence=count;r.credit_bytes=endpoint->budget.max_frame_bytes;
    if(submit(&r,&calls,endpoint->budget.max_requests,call_id,&reply)!=MP_CODEC_OK||mp_channel_response_get(reply,&v,sizeof(v))!=MP_CODEC_OK)goto done;
    if(v.status==MP_CHANNEL_CLOSED)break;
    if(v.status==MP_CHANNEL_IDLE||v.status==MP_CHANNEL_CLOSING_UNCONFIRMED){mp_channel_response_free(reply);reply=NULL;continue;}
    if(v.status!=MP_CHANNEL_FRAME||!v.has_frame||v.sequence!=count+1||v.sequence>endpoint->budget.max_messages)goto done;
    if(v.bytes.length>endpoint->budget.max_bytes-total)goto done;
    total+=v.bytes.length;
    if(mc_metadata_v1_decode(v.bytes.data,v.bytes.length,endpoint->source_epoch.data,v.cursor.data,v.cursor.length,&metadata,sizeof(metadata))!=MC_METADATA_V1_OK)goto done;
    if(count&&memcmp(scope,metadata.scope_digest,32)!=0)goto done;
    memcpy(scope,metadata.scope_digest,32);
    r.kind=MP_CHANNEL_ACK;r.sequence=v.sequence;r.frame_sha256=v.frame_sha256;r.cursor=v.cursor;
    /* Keep borrowed frame fields alive through the exact durable ACK call. */
    if(submit(&r,&calls,endpoint->budget.max_requests,call_id,&ack)!=MP_CODEC_OK||mp_channel_response_get(ack,&a,sizeof(a))!=MP_CODEC_OK||a.status!=MP_CHANNEL_ACKED||a.last_acked!=v.sequence)goto done;
    count=v.sequence;mp_channel_response_free(ack);ack=NULL;mp_channel_response_free(reply);reply=NULL;
    r.frame_sha256.data=NULL;r.frame_sha256.length=0;r.cursor.data=NULL;r.cursor.length=0;
  }
  put64(summary,count);
  if(mp_task_output(task,summary,8,completion,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
  result=mp_wasm_task_complete(completion,length);
done:
  mp_channel_response_free(ack);mp_channel_response_free(reply);mp_channel_directory_free(directory);mp_task_free(task);free(completion);free(input);return result;
}
