// NEW fixture: C++ ownership wrappers over the unmodified public channel SDK.
#include "morrow_channel_v1.hpp"
#include "morrow_plugin_task.h"
#include "payloads.h"
#include <cstring>
using morrow::channel_v1::request;
using morrow::channel_v1::response;
static void put32(uint8_t *p,uint32_t n){for(uint32_t i=0;i<4;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static bool equals(mp_span v,const char *s){size_t n=std::strlen(s);return v.length==n&&std::memcmp(v.data,s,n)==0;}
struct task_owner{mp_task *value=nullptr;~task_owner(){mp_task_free(value);}};
struct digest_owner{mp_channel_digest *value=nullptr;~digest_owner(){mp_channel_digest_free(value);}};
extern "C" int32_t morrow_run(void){
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES),completion(MP_MAX_TASK_BYTES);task_owner task;digest_owner digest;
  int32_t n=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
  if(n<=0||static_cast<uint32_t>(n)>MP_MAX_TASK_BYTES||mp_task_decode(input.data(),static_cast<uint32_t>(n),&task.value)!=MP_CODEC_OK)return -1;
  mp_transform_view t{};
  if(mp_task_get_transform(task.value,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"channel.ws.duplex")||!equals(t.input_type,"bytes")||!equals(t.output_type,"bytes")||t.input.length!=65||t.input.data[0]!=3)return -1;
  request r;std::copy_n(t.input.data+1,32,r.reference.begin());std::copy_n(t.input.data+33,32,r.source_epoch.begin());
  uint64_t calls=0,total=0,last_acked=0;
  if(mp_channel_digest_new(&digest.value)!=MP_CODEC_OK)return -1;
  auto submit=[&](){r.call_id.fill(0x57);put64(r.call_id.data(),++calls);return response::call(r);};
  for(uint32_t i=0;i<3;i++){
    r.kind=MP_CHANNEL_SEND;r.sequence=i+1;r.bytes.assign(ws_payloads[i],ws_payloads[i]+ws_lengths[i]);
    {auto sent=submit();mp_channel_response_view v{};if(sent.view(v)!=MP_CODEC_OK||v.status!=MP_CHANNEL_ACCEPTED)return -1;}
    r.kind=MP_CHANNEL_RECEIVE;r.sequence=last_acked;r.credit_bytes=32768;
    auto received=submit();mp_channel_response_view v{};
    if(received.view(v)!=MP_CODEC_OK||v.status!=MP_CHANNEL_FRAME||!v.has_frame||v.sequence!=i+1||v.bytes.length!=ws_lengths[i]||std::memcmp(v.bytes.data,ws_payloads[i],v.bytes.length)!=0)return -1;
    if(mp_channel_digest_update(digest.value,v.bytes.data,v.bytes.length)!=MP_CODEC_OK)return -1;
    total+=v.bytes.length;r.kind=MP_CHANNEL_ACK;r.sequence=v.sequence;
    std::copy_n(v.frame_sha256.data,32,r.frame_sha256.begin());r.cursor.assign(v.cursor.data,v.cursor.data+v.cursor.length);
    auto ack=submit();mp_channel_response_view a{};if(ack.view(a)!=MP_CODEC_OK||a.status!=MP_CHANNEL_ACKED)return -1;
    last_acked=v.sequence;
  }
  r.kind=MP_CHANNEL_RECEIVE;r.sequence=last_acked;r.credit_bytes=32768;
  auto terminal=submit();mp_channel_response_view v{};if(terminal.view(v)!=MP_CODEC_OK||v.status!=MP_CHANNEL_CLOSED)return -1;
  std::array<uint8_t,64> summary{};std::memcpy(summary.data(),"WSV1",4);put32(summary.data()+4,3);put32(summary.data()+8,v.status);put32(summary.data()+12,v.resource_reclaimed);put64(summary.data()+16,3);put64(summary.data()+24,total);
  uint32_t length=0;
  if(mp_channel_digest_finish(digest.value,summary.data()+32,32)!=MP_CODEC_OK||mp_task_output(task.value,summary.data(),64,completion.data(),MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)return -1;
  return mp_wasm_task_complete(completion.data(),length);
}
