#include "morrow_fs_directory_request_v1.hpp"
#include "morrow_fs_directory_v1.hpp"
extern "C" {
#if defined(__wasm__)
__attribute__((import_module("morrow_task_v1"),import_name("read_input")))
#endif
int32_t task_read(uint8_t*,uint32_t);
#if defined(__wasm__)
__attribute__((import_module("morrow_task_v1"),import_name("complete")))
#endif
int32_t task_complete(const uint8_t*,uint32_t);
}
constexpr uint32_t TASK_BUFFER_BYTES=131072;
#include <array>
#include <cstring>
namespace sdk=morrow::fs_directory_request_v1;
static void put64(uint8_t *p,uint64_t n){for(unsigned i=0;i<8;i++)p[i]=static_cast<uint8_t>(n>>(i*8));}
static int32_t run(unsigned mode){
 std::vector<uint8_t> input(TASK_BUFFER_BYTES),completion;
 int32_t n=task_read(input.data(),TASK_BUFFER_BYTES);
 sdk::request open;mdr_request_view original{};
 if(n<=0||n>512||open.decode(input.data(),static_cast<uint32_t>(n))!=MDR_OK||open.view(original)!=MDR_OK||original.action!=MDR_OPEN)return -1;
 sdk::client state;sdk::response response;mdr_response_view r{};
 if(state.initialize(original.nomination_ref)!=MDR_OK||state.call_once(open,response)!=MDR_OK||response.view(r)!=MDR_OK||r.reply.kind!=MDR_REPLY_OPENED)return -1;
 mdr_request_view q{};q.abi_version=1;q.struct_size=sizeof(q);std::memcpy(q.nomination_ref,original.nomination_ref,32);std::memcpy(q.selection_epoch,r.reply.selection_epoch,32);q.page_sequence=1;
 uint64_t serial=0;
 for(;;){
  do {if(++serial>1024)return -1;std::memset(q.nonce,0xd1,32);put64(q.nonce,serial);}while(std::memcmp(q.nonce,original.nonce,32)==0);
  q.action=mode==1?MDR_FINISH:mode==2?MDR_CANCEL:MDR_NEXT;
  if(mode){q.page_sequence=0;q.has_after_entry_id=0;std::memset(q.after_entry_id,0,32);}
  if(state.call_once(q,response)!=MDR_OK||response.view(r)!=MDR_OK)return -1;
  if(mode){if(r.reply.kind!=(mode==1?MDR_REPLY_FINISHED:MDR_REPLY_CANCELLED))return -1;break;}
  morrow::fs_directory_v1::page decoded;mfd_page_view_v1 p{};
  if(r.reply.kind!=MDR_REPLY_PAGE||decoded.decode(r.reply.page_wire,r.reply.page_wire_length)!=MFD_OK||decoded.view(p)!=MFD_OK)return -1;
  q.has_after_entry_id=0;std::memset(q.after_entry_id,0,32);
  for(uint32_t i=0;i<p.entry_count;i++){mfd_entry_view_v1 e{};if(decoded.entry_view(i,e)!=MFD_OK)return -1;
   if(i+1==p.entry_count){q.has_after_entry_id=1;std::memcpy(q.after_entry_id,e.entry_id,32);}}
  if(p.terminal)break;if(p.page_sequence==UINT64_MAX)return -1;q.page_sequence=p.page_sequence+1;
 }
 mdr_snapshot snapshot{};if(state.snapshot(snapshot)!=MDR_OK||snapshot.phase!=MDR_TERMINAL)return -1;
 if(response.encode(completion)!=MDR_OK)return -1;
 return task_complete(completion.data(),static_cast<uint32_t>(completion.size()));
}
extern "C" int32_t morrow_run(){return run(0);}
extern "C" int32_t morrow_finish(){return run(1);}
extern "C" int32_t morrow_cancel(){return run(2);}
