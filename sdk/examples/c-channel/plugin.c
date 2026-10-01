/* Task input is a mode and two opaque IDs. No paths/sockets or grants. */
#include "morrow_channel_v1.h"
#include "morrow_plugin_task.h"
#include <stdlib.h>
#include <string.h>
static void put32(uint8_t *p,uint32_t n){for(uint32_t i=0;i<4;i++)p[i]=(uint8_t)(n>>(8*i));}
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=(uint8_t)(n>>(8*i));}
static int equals(mp_span v,const char *text){size_t n=strlen(text);return v.length==n&&memcmp(v.data,text,n)==0;}
static uint32_t submit(mp_channel_request_v1 *r,uint64_t *calls,uint8_t call_id[32],mp_channel_response **response){
  memset(call_id,0x43,32);put64(call_id,++*calls);r->call_id.data=call_id;r->call_id.length=32;
  return mp_wasm_channel_call(r,response);
}
int32_t morrow_run(void){
  uint8_t *input=NULL,*completion=NULL,*payload=NULL;mp_task *task=NULL;mp_channel_response *response=NULL,*ack=NULL;
  mp_channel_digest *digest=NULL;mp_transform_view t;mp_channel_response_view v={0},av={0};
  uint8_t call_id[32],summary[64]={0};uint32_t completion_length=0,status=MP_CHANNEL_READY,reclaimed=0,mode;
  uint64_t calls=0,count=0,total=0,last_acked=0;int32_t result=-1;
  mp_channel_request_v1 r={0};r.abi_version=1;r.struct_size=sizeof(r);
  input=(uint8_t*)malloc(MP_MAX_TASK_BYTES);completion=(uint8_t*)malloc(MP_MAX_TASK_BYTES);payload=(uint8_t*)malloc(32768);
  if(!input||!completion||!payload)goto done;
  int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);
  if(n<=0||(uint32_t)n>MP_MAX_TASK_BYTES||mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
  if(mp_task_get_transform(task,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"channel.exercise")||!equals(t.input_type,"bytes")||!equals(t.output_type,"bytes")||t.input.length!=65||t.input.data[0]>2)goto done;
  mode=t.input.data[0];r.reference.data=t.input.data+1;r.reference.length=32;r.source_epoch.data=t.input.data+33;r.source_epoch.length=32;
  if(mp_channel_digest_new(&digest)!=MP_CODEC_OK)goto done;
  if(mode==1){
    for(uint64_t seq=1;seq<=5;seq++){
      for(uint32_t i=0;i<32768;i++)payload[i]=(uint8_t)((i+seq*17)%251);
      r.kind=MP_CHANNEL_SEND;r.sequence=seq;r.bytes.data=payload;r.bytes.length=32768;
      if(submit(&r,&calls,call_id,&response)!=MP_CODEC_OK||mp_channel_response_get(response,&v,sizeof(v))!=MP_CODEC_OK)goto done;
      status=v.status;reclaimed=v.resource_reclaimed;
      mp_channel_response_free(response);response=NULL;
      if(status!=MP_CHANNEL_ACCEPTED)break;
      if(mp_channel_digest_update(digest,payload,32768)!=MP_CODEC_OK)goto done;
      count++;total+=32768;
    }
  }else{
    for(uint32_t i=0;i<32;i++){
      r.kind=MP_CHANNEL_RECEIVE;r.sequence=last_acked;r.credit_bytes=65536;
      if(submit(&r,&calls,call_id,&response)!=MP_CODEC_OK||mp_channel_response_get(response,&v,sizeof(v))!=MP_CODEC_OK)goto done;
      status=v.status;reclaimed=v.resource_reclaimed;
      if(status!=MP_CHANNEL_FRAME){mp_channel_response_free(response);response=NULL;break;}
      if(!v.has_frame||mp_channel_digest_update(digest,v.bytes.data,v.bytes.length)!=MP_CODEC_OK)goto done;
      count++;total+=v.bytes.length;
      r.kind=MP_CHANNEL_ACK;r.sequence=v.sequence;r.frame_sha256=v.frame_sha256;r.cursor=v.cursor;
      /* Keep all borrowed ACK fields alive until submission has copied them. */
      if(submit(&r,&calls,call_id,&ack)!=MP_CODEC_OK||mp_channel_response_get(ack,&av,sizeof(av))!=MP_CODEC_OK)goto done;
      status=av.status;reclaimed=av.resource_reclaimed;last_acked=v.sequence;
      mp_channel_response_free(ack);ack=NULL;mp_channel_response_free(response);response=NULL;
      r.frame_sha256.data=NULL;r.frame_sha256.length=0;r.cursor.data=NULL;r.cursor.length=0;
      if(status!=MP_CHANNEL_ACKED)break;
    }
  }
  if(status==MP_CHANNEL_ACCEPTED||status==MP_CHANNEL_ACKED||status==MP_CHANNEL_IDLE||status==MP_CHANNEL_READY){
    r.kind=MP_CHANNEL_CLOSE;
    if(submit(&r,&calls,call_id,&response)!=MP_CODEC_OK||mp_channel_response_get(response,&v,sizeof(v))!=MP_CODEC_OK)goto done;
    status=v.status;reclaimed=v.resource_reclaimed;
  }
  memcpy(summary,"CHV1",4);put32(summary+4,mode);put32(summary+8,status);put32(summary+12,reclaimed);put64(summary+16,count);put64(summary+24,total);
  if(mp_channel_digest_finish(digest,summary+32,32)!=MP_CODEC_OK||mp_task_output(task,summary,sizeof(summary),completion,MP_MAX_TASK_BYTES,&completion_length)!=MP_CODEC_OK)goto done;
  result=mp_wasm_task_complete(completion,completion_length);
done:
  mp_channel_response_free(ack);mp_channel_response_free(response);mp_channel_digest_free(digest);mp_task_free(task);
  free(payload);free(completion);free(input);return result;
}
