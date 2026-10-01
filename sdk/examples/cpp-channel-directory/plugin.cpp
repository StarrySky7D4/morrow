#include "morrow_channel_v1.hpp"
#include "morrow_plugin_task.h"
#include <cstring>
using morrow::channel_v1::directory;
using morrow::channel_v1::request;
using morrow::channel_v1::response;
static void put32(uint8_t *p,uint32_t n){for(uint32_t i=0;i<4;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static void put64(uint8_t *p,uint64_t n){for(uint32_t i=0;i<8;i++)p[i]=static_cast<uint8_t>(n>>(8*i));}
static bool equals(mp_span value,const char *text){size_t n=std::strlen(text);return value.length==n&&std::memcmp(value.data,text,n)==0;}
struct task_owner{mp_task *value=nullptr;~task_owner(){mp_task_free(value);}};
struct digest_owner{mp_channel_digest *value=nullptr;~digest_owner(){mp_channel_digest_free(value);}};
static bool next_id(request& operation,const mp_channel_directory_view& metadata,uint64_t& calls){
  static const uint8_t domain[]="morrow.channel.directory.request.v1";
  digest_owner id;std::array<uint8_t,8> counter{};put64(counter.data(),++calls);
  return mp_channel_digest_new(&id.value)==MP_CODEC_OK&&mp_channel_digest_update(id.value,domain,static_cast<uint32_t>(sizeof(domain)-1))==MP_CODEC_OK
      &&mp_channel_digest_update(id.value,metadata.scope_sha256.data,metadata.scope_sha256.length)==MP_CODEC_OK
      &&mp_channel_digest_update(id.value,operation.reference.data(),32)==MP_CODEC_OK&&mp_channel_digest_update(id.value,operation.source_epoch.data(),32)==MP_CODEC_OK
      &&mp_channel_digest_update(id.value,counter.data(),8)==MP_CODEC_OK&&mp_channel_digest_finish(id.value,operation.call_id.data(),32)==MP_CODEC_OK;
}
extern "C" int32_t morrow_run(void){
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES),completion(MP_MAX_TASK_BYTES);task_owner task;digest_owner digest;mp_transform_view transform{};
  int32_t n=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
  if(n<=0||static_cast<uint32_t>(n)>MP_MAX_TASK_BYTES||mp_task_decode(input.data(),static_cast<uint32_t>(n),&task.value)!=MP_CODEC_OK)return -1;
  if(mp_task_get_transform(task.value,&transform,sizeof(transform))!=MP_CODEC_OK||!equals(transform.handler,"channel.directory.consume")||!equals(transform.input_type,"morrow.channel.directory.v1")||!equals(transform.output_type,"bytes"))return -1;
  if(!transform.input.length||transform.input.length>MP_MAX_TASK_VALUE_BYTES)return -1;
  std::vector<uint8_t> encoded(transform.input.data,transform.input.data+transform.input.length);auto metadata=directory::decode(encoded);mp_channel_directory_view d{};
  if(metadata.view(d)!=MP_CODEC_OK||d.channel_count!=1)return -1;
  const auto& endpoint=d.channels[0];if(endpoint.kind>1)return -1;uint32_t mode=endpoint.kind==0?0:2,status=MP_CHANNEL_READY,reclaimed=0;
  request operation;std::copy_n(endpoint.reference.data,32,operation.reference.begin());std::copy_n(endpoint.source_epoch.data,32,operation.source_epoch.begin());
  uint64_t maximum=std::min<uint64_t>(32,endpoint.budget.max_messages);maximum=std::min(maximum,(endpoint.budget.max_requests-1)/2);
  uint64_t calls=0,count=0,total=0,last_acked=0;if(mp_channel_digest_new(&digest.value)!=MP_CODEC_OK)return -1;
  for(uint64_t i=0;i<maximum;i++){
    operation.kind=MP_CHANNEL_RECEIVE;operation.sequence=last_acked;operation.credit_bytes=endpoint.budget.max_frame_bytes;if(!next_id(operation,d,calls))return -1;
    auto reply=response::call(operation);mp_channel_response_view value{};if(reply.view(value)!=MP_CODEC_OK)return -1;
    status=value.status;reclaimed=value.resource_reclaimed;if(status!=MP_CHANNEL_FRAME)break;
    if(!value.has_frame||mp_channel_digest_update(digest.value,value.bytes.data,value.bytes.length)!=MP_CODEC_OK)return -1;count++;total+=value.bytes.length;
    operation.kind=MP_CHANNEL_ACK;operation.sequence=value.sequence;std::copy_n(value.frame_sha256.data,32,operation.frame_sha256.begin());
    if(value.cursor.length)operation.cursor.assign(value.cursor.data,value.cursor.data+value.cursor.length);else operation.cursor.clear();if(!next_id(operation,d,calls))return -1;
    auto ack=response::call(operation);mp_channel_response_view receipt{};if(ack.view(receipt)!=MP_CODEC_OK)return -1;
    status=receipt.status;reclaimed=receipt.resource_reclaimed;last_acked=value.sequence;if(status!=MP_CHANNEL_ACKED)break;
  }
  if(status==MP_CHANNEL_READY||status==MP_CHANNEL_IDLE||status==MP_CHANNEL_ACKED){
    operation.kind=MP_CHANNEL_CLOSE;if(!next_id(operation,d,calls))return -1;auto reply=response::call(operation);mp_channel_response_view value{};
    if(reply.view(value)!=MP_CODEC_OK)return -1;status=value.status;reclaimed=value.resource_reclaimed;
  }
  std::array<uint8_t,64> summary{};std::memcpy(summary.data(),"CHV1",4);put32(summary.data()+4,mode);put32(summary.data()+8,status);put32(summary.data()+12,reclaimed);put64(summary.data()+16,count);put64(summary.data()+24,total);
  uint32_t length=0;if(mp_channel_digest_finish(digest.value,summary.data()+32,32)!=MP_CODEC_OK||mp_task_output(task.value,summary.data(),64,completion.data(),MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)return -1;
  return mp_wasm_task_complete(completion.data(),length);
}
