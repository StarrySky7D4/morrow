// Original C++ ownership wrappers; metadata decode is the independent extension.
#include "morrow_channel_v1.hpp"
#include "morrow_plugin_task.h"
#include "morrow_changes_metadata_v1.hpp"
#include <cstring>
using morrow::channel_v1::request;
using morrow::channel_v1::response;
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static bool equals(mp_span v,const char *s){size_t n=std::strlen(s);return v.length==n&&std::memcmp(v.data,s,n)==0;}
struct task_owner{mp_task *value=nullptr;~task_owner(){mp_task_free(value);}};
extern "C" int32_t morrow_run(void){
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES),completion(MP_MAX_TASK_BYTES);task_owner task;
  int32_t n=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
  if(n<=0||static_cast<uint32_t>(n)>MP_MAX_TASK_BYTES||mp_task_decode(input.data(),static_cast<uint32_t>(n),&task.value)!=MP_CODEC_OK)return -1;
  mp_transform_view t{};
  if(mp_task_get_transform(task.value,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.handler,"changes.metadata")||!equals(t.input_type,"morrow.channel.directory.v1")||!equals(t.output_type,"morrow.changes.metadata.count.v1"))return -1;
  auto directory=morrow::channel_v1::directory::decode(std::vector<uint8_t>(t.input.data,t.input.data+t.input.length));mp_channel_directory_view d{};
  if(directory.view(d)!=MP_CODEC_OK||d.channel_count!=1||d.channels[0].kind!=1)return -1;
  const auto& endpoint=d.channels[0];
  request r;std::copy_n(endpoint.reference.data,32,r.reference.begin());std::copy_n(endpoint.source_epoch.data,32,r.source_epoch.begin());
  uint64_t calls=0,total=0,count=0;std::array<uint8_t,32> scope{};
  auto submit=[&](){r.call_id.fill(0x47);put64(r.call_id.data(),++calls);return response::call(r);};
  for(;;){
    if(calls>=endpoint.budget.max_requests)return -1;
    r.kind=MP_CHANNEL_RECEIVE;r.sequence=count;r.credit_bytes=endpoint.budget.max_frame_bytes;
    auto received=submit();mp_channel_response_view v{};
    if(received.view(v)!=MP_CODEC_OK)return -1;
    if(v.status==MP_CHANNEL_CLOSED)break;
    if(v.status==MP_CHANNEL_IDLE||v.status==MP_CHANNEL_CLOSING_UNCONFIRMED)continue;
    if(v.status!=MP_CHANNEL_FRAME||!v.has_frame||v.sequence!=count+1||v.sequence>endpoint.budget.max_messages)return -1;
    if(v.bytes.length>endpoint.budget.max_bytes-total)return -1;
    total+=v.bytes.length;
    morrow::changes_metadata_v1::metadata metadata;
    if(metadata.decode(v.bytes.data,v.bytes.length,endpoint.source_epoch.data,v.cursor.data,v.cursor.length)!=MC_METADATA_V1_OK)return -1;
    if(count&&std::memcmp(scope.data(),metadata.value.scope_digest,32)!=0)return -1;
    std::copy_n(metadata.value.scope_digest,32,scope.begin());
    r.kind=MP_CHANNEL_ACK;r.sequence=v.sequence;
    std::copy_n(v.frame_sha256.data,32,r.frame_sha256.begin());r.cursor.assign(v.cursor.data,v.cursor.data+v.cursor.length);
    if(calls>=endpoint.budget.max_requests)return -1;
    auto ack=submit();mp_channel_response_view a{};if(ack.view(a)!=MP_CODEC_OK||a.status!=MP_CHANNEL_ACKED||a.last_acked!=v.sequence)return -1;
    count=v.sequence;
  }
  std::array<uint8_t,8> summary{};put64(summary.data(),count);uint32_t length=0;
  if(mp_task_output(task.value,summary.data(),8,completion.data(),MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)return -1;
  return mp_wasm_task_complete(completion.data(),length);
}
