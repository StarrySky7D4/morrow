/* Typed full SSE event metadata; unchanged nonduplex channel transport, no reconnect grant. */
#include "morrow_channel_v1.h"
#include "morrow_plugin_task.h"
#include "morrow_sse_event_v1.h"
#include <stdlib.h>
#include <string.h>
static const uint8_t data0[]={228,189,160,10,115,101,99,111,110,100};
static const uint8_t data2[]={230,156,128,231,187,136,32,240,159,153,130};
static const uint8_t event2[]={230,155,180,230,150,176};
static const uint8_t *expected_data[]={data0,(const uint8_t*)"",data2,(const uint8_t*)"[DONE]",(const uint8_t*)"after-DONE"};
static const uint32_t expected_length[]={sizeof(data0),0,sizeof(data2),6,10};
static const uint8_t *expected_event[]={(const uint8_t*)"delta",(const uint8_t*)"message",event2,(const uint8_t*)"message",(const uint8_t*)"message"};
static const uint32_t expected_event_length[]={5,7,sizeof(event2),7,7};
static int semantic(const mse_event_v1 *v,uint32_t i){
 if(v->data_length!=expected_length[i]||memcmp(v->storage+v->data_offset,expected_data[i],v->data_length)!=0
    ||v->event_length!=expected_event_length[i]||memcmp(v->storage+v->event_offset,expected_event[i],v->event_length)!=0
    ||v->has_retry!=(i!=0)||v->retry!=(i<2?0:UINT64_MAX)||v->id_length!=(i==0?300u:0u))return 0;
 for(uint32_t k=0;k<v->id_length;k++)if(v->storage[v->id_offset+k]!='i')return 0;
 return 1;
}
static void put32(uint8_t *p,uint32_t n){for(uint32_t i=0;i<4;i++)p[i]=(uint8_t)(n>>(8*i));}
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=(uint8_t)(n>>(8*i));}
static int equals(mp_span v,const char *s){size_t n=strlen(s);return v.length==n&&memcmp(v.data,s,n)==0;}
static uint32_t submit(mp_channel_request_v1 *r,uint64_t *calls,uint8_t call_id[32],mp_channel_response **reply){
  if(*calls>=1024)return MP_CODEC_LIMIT;
  memset(call_id,0x57,32);put64(call_id,++*calls);r->call_id.data=call_id;r->call_id.length=32;
  return mp_wasm_channel_call(r,reply);
}
/* Poll only original channel delivery; never retry a socket operation or ACK. */
static uint32_t receive(mp_channel_request_v1 *r,uint64_t *calls,uint8_t call_id[32],mp_channel_response **reply,mp_channel_response_view *view){
  for(;;){
    uint32_t code=submit(r,calls,call_id,reply);if(code!=MP_CODEC_OK)return code;
    code=mp_channel_response_get(*reply,view,sizeof(*view));if(code!=MP_CODEC_OK)return code;
    if(view->status!=MP_CHANNEL_IDLE&&view->status!=MP_CHANNEL_CLOSING_UNCONFIRMED)return MP_CODEC_OK;
    mp_channel_response_free(*reply);*reply=NULL;
  }
}
int32_t morrow_run(void){
  uint8_t *input=NULL,*completion=NULL;mp_task *task=NULL;mp_channel_response *reply=NULL,*ack=NULL;
  mp_channel_digest *digest=NULL;mp_transform_view t;mp_channel_response_view v={0},a={0};
  uint8_t call_id[32],summary[64]={0};mse_event_v1 decoded;uint32_t length=0;uint64_t calls=0,total=0,last_acked=0;int32_t result=-1;
  mp_channel_request_v1 r={0};r.abi_version=1;r.struct_size=sizeof(r);
  input=(uint8_t*)malloc(MP_MAX_TASK_BYTES);completion=(uint8_t*)malloc(MP_MAX_TASK_BYTES);
  if(!input||!completion)goto done;
  int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);
  if(n<=0||(uint32_t)n>MP_MAX_TASK_BYTES||mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
  if(mp_task_get_transform(task,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"channel.sse.sdk")||!equals(t.input_type,"bytes")||!equals(t.output_type,"bytes")||t.input.length!=65||t.input.data[0]!=5)goto done;
  r.reference.data=t.input.data+1;r.reference.length=32;r.source_epoch.data=t.input.data+33;r.source_epoch.length=32;
  if(mp_channel_digest_new(&digest)!=MP_CODEC_OK)goto done;
  for(uint32_t i=0;i<5;i++){
    r.kind=MP_CHANNEL_RECEIVE;r.sequence=last_acked;r.credit_bytes=32768;
    if(receive(&r,&calls,call_id,&reply,&v)!=MP_CODEC_OK||v.status!=MP_CHANNEL_FRAME||!v.has_frame)goto done;
    if(v.sequence!=i+1||mse_event_v1_decode(v.bytes.data,v.bytes.length,&decoded,sizeof(decoded))!=0||!semantic(&decoded,i))goto done;
    if(mp_channel_digest_update(digest,v.bytes.data,v.bytes.length)!=MP_CODEC_OK)goto done;
    total+=v.bytes.length;
    r.kind=MP_CHANNEL_ACK;r.sequence=v.sequence;r.frame_sha256=v.frame_sha256;r.cursor=v.cursor;
    /* Keep borrowed frame fields alive through the exact ACK call. */
    if(submit(&r,&calls,call_id,&ack)!=MP_CODEC_OK||mp_channel_response_get(ack,&a,sizeof(a))!=MP_CODEC_OK||a.status!=MP_CHANNEL_ACKED||a.last_acked!=v.sequence)goto done;
    last_acked=v.sequence;mp_channel_response_free(ack);ack=NULL;mp_channel_response_free(reply);reply=NULL;
    r.frame_sha256.data=NULL;r.frame_sha256.length=0;r.cursor.data=NULL;r.cursor.length=0;
  }
  r.kind=MP_CHANNEL_RECEIVE;r.sequence=last_acked;r.credit_bytes=32768;
  if(receive(&r,&calls,call_id,&reply,&v)!=MP_CODEC_OK||v.status!=MP_CHANNEL_CLOSED)goto done;
  memcpy(summary,"SES1",4);put32(summary+4,5);put32(summary+8,v.status);put32(summary+12,v.resource_reclaimed);put64(summary+16,5);put64(summary+24,total);
  if(mp_channel_digest_finish(digest,summary+32,32)!=MP_CODEC_OK||mp_task_output(task,summary,64,completion,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
  result=mp_wasm_task_complete(completion,length);
done:
  mp_channel_response_free(ack);mp_channel_response_free(reply);mp_channel_digest_free(digest);mp_task_free(task);free(completion);free(input);return result;
}
