#include "morrow_channel_v1.hpp"
#include "morrow_plugin_task.h"
#include <cstring>
using morrow::channel_v1::request;
using morrow::channel_v1::response;
static void put32(uint8_t *p,uint32_t n){for(uint32_t i=0;i<4;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static bool equals(mp_span v,const char *text){size_t n=std::strlen(text);return v.length==n&&std::memcmp(v.data,text,n)==0;}
struct task_owner {mp_task* value=nullptr;~task_owner(){mp_task_free(value);}};
struct digest_owner {mp_channel_digest* value=nullptr;~digest_owner(){mp_channel_digest_free(value);}};
extern "C" int32_t morrow_run(void){
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES),completion(MP_MAX_TASK_BYTES);task_owner task;digest_owner digest;
  int32_t n=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
  if(n<=0||static_cast<uint32_t>(n)>MP_MAX_TASK_BYTES||mp_task_decode(input.data(),static_cast<uint32_t>(n),&task.value)!=MP_CODEC_OK)return -1;
  mp_transform_view t{};
  if(mp_task_get_transform(task.value,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"channel.exercise")||!equals(t.input_type,"bytes")||!equals(t.output_type,"bytes")||t.input.length!=65||t.input.data[0]>2)return -1;
  uint32_t mode=t.input.data[0],status=MP_CHANNEL_READY,reclaimed=0;uint64_t calls=0,count=0,total=0,last_acked=0;
  request r;std::copy_n(t.input.data+1,32,r.reference.begin());std::copy_n(t.input.data+33,32,r.source_epoch.begin());
  if(mp_channel_digest_new(&digest.value)!=MP_CODEC_OK)return -1;
  auto submit=[&](){r.call_id.fill(0x43);put64(r.call_id.data(),++calls);return response::call(r);};
  if(mode==1){
    r.bytes.resize(32768);
    for(uint64_t seq=1;seq<=5;seq++){
      for(uint32_t i=0;i<32768;i++)r.bytes[i]=static_cast<uint8_t>((i+seq*17)%251);
      r.kind=MP_CHANNEL_SEND;r.sequence=seq;auto reply=submit();mp_channel_response_view v{};
      if(reply.view(v)!=MP_CODEC_OK)return -1;status=v.status;reclaimed=v.resource_reclaimed;if(status!=MP_CHANNEL_ACCEPTED)break;
      if(mp_channel_digest_update(digest.value,r.bytes.data(),static_cast<uint32_t>(r.bytes.size()))!=MP_CODEC_OK)return -1;
      count++;total+=r.bytes.size();
    }
  }else{
    for(uint32_t i=0;i<32;i++){
      r.kind=MP_CHANNEL_RECEIVE;r.sequence=last_acked;r.credit_bytes=65536;auto reply=submit();mp_channel_response_view v{};
      if(reply.view(v)!=MP_CODEC_OK)return -1;status=v.status;reclaimed=v.resource_reclaimed;if(status!=MP_CHANNEL_FRAME)break;
      if(!v.has_frame||mp_channel_digest_update(digest.value,v.bytes.data,v.bytes.length)!=MP_CODEC_OK)return -1;
      count++;total+=v.bytes.length;r.kind=MP_CHANNEL_ACK;r.sequence=v.sequence;
      std::copy_n(v.frame_sha256.data,32,r.frame_sha256.begin());
      if(v.cursor.length)r.cursor.assign(v.cursor.data,v.cursor.data+v.cursor.length);else r.cursor.clear();
      auto ack=submit();mp_channel_response_view av{};if(ack.view(av)!=MP_CODEC_OK)return -1;
      status=av.status;reclaimed=av.resource_reclaimed;last_acked=v.sequence;if(status!=MP_CHANNEL_ACKED)break;
    }
  }
  if(status==MP_CHANNEL_ACCEPTED||status==MP_CHANNEL_ACKED||status==MP_CHANNEL_IDLE||status==MP_CHANNEL_READY){
    r.kind=MP_CHANNEL_CLOSE;auto reply=submit();mp_channel_response_view v{};if(reply.view(v)!=MP_CODEC_OK)return -1;status=v.status;reclaimed=v.resource_reclaimed;
  }
  std::array<uint8_t,64> summary{};std::memcpy(summary.data(),"CHV1",4);put32(summary.data()+4,mode);put32(summary.data()+8,status);put32(summary.data()+12,reclaimed);put64(summary.data()+16,count);put64(summary.data()+24,total);
  uint32_t length=0;
  if(mp_channel_digest_finish(digest.value,summary.data()+32,32)!=MP_CODEC_OK||mp_task_output(task.value,summary.data(),64,completion.data(),MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)return -1;
  return mp_wasm_task_complete(completion.data(),length);
}
