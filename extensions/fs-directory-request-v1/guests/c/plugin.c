/* Typed directory requests and the unchanged typed page decoder. No path ABI. */
#include "morrow_fs_directory_request_v1.h"
#include "morrow_fs_directory_v1.h"
#if defined(__wasm__)
__attribute__((import_module("morrow_task_v1"),import_name("read_input")))
#endif
extern int32_t task_read(uint8_t*,uint32_t);
#if defined(__wasm__)
__attribute__((import_module("morrow_task_v1"),import_name("complete")))
#endif
extern int32_t task_complete(const uint8_t*,uint32_t);
#define TASK_BUFFER_BYTES 131072u
#include <stdlib.h>
#include <string.h>
static void put64(uint8_t *p,uint64_t n){for(unsigned i=0;i<8;i++)p[i]=(uint8_t)(n>>(i*8));}
static int32_t run(uint32_t mode){
 uint8_t *input=(uint8_t*)malloc(TASK_BUFFER_BYTES),*completion=(uint8_t*)malloc(TASK_BUFFER_BYTES);
 mdr_request *open=NULL;mdr_response *response=NULL;mdr_client *client=NULL;mfd_page_v1 *page=NULL;
 mdr_request_view q={0},original={0};mdr_response_view r={0};
 uint64_t serial=0;uint32_t length=0;int32_t result=-1;
 if(!input||!completion)goto done;
 int32_t n=task_read(input,TASK_BUFFER_BYTES);
 if(n<=0||n>512||mdr_request_decode(input,(uint32_t)n,&open)!=MDR_OK||mdr_request_get_view(open,&original,sizeof(original))!=MDR_OK||original.action!=MDR_OPEN)goto done;
 if(mdr_client_new(original.nomination_ref,32,&client)!=MDR_OK||mdr_client_call_request_once(client,open,NULL,NULL,&response)!=MDR_OK||mdr_response_get_view(response,&r,sizeof(r))!=MDR_OK||r.reply.kind!=MDR_REPLY_OPENED)goto done;
 q.abi_version=1;q.struct_size=sizeof(q);memcpy(q.nomination_ref,original.nomination_ref,32);memcpy(q.selection_epoch,r.reply.selection_epoch,32);q.page_sequence=1;
 mdr_response_free(response);response=NULL;
 for(;;){
  do {if(++serial>1024)goto done;memset(q.nonce,0xd1,32);put64(q.nonce,serial);}while(memcmp(q.nonce,original.nonce,32)==0);
  q.action=mode==1?MDR_FINISH:mode==2?MDR_CANCEL:MDR_NEXT;
  if(mode){q.page_sequence=0;q.has_after_entry_id=0;memset(q.after_entry_id,0,32);}
  if(mdr_client_call_once(client,&q,sizeof(q),NULL,NULL,&response)!=MDR_OK||mdr_response_get_view(response,&r,sizeof(r))!=MDR_OK)goto done;
  if(mode){if(r.reply.kind!=(mode==1?MDR_REPLY_FINISHED:MDR_REPLY_CANCELLED))goto done;break;}
  if(r.reply.kind!=MDR_REPLY_PAGE||mfd_page_v1_decode(r.reply.page_wire,r.reply.page_wire_length,&page)!=MFD_OK)goto done;
  mfd_page_view_v1 p={0};if(mfd_page_v1_view(page,&p,sizeof(p))!=MFD_OK)goto done;
  q.has_after_entry_id=0;memset(q.after_entry_id,0,32);
  for(uint32_t i=0;i<p.entry_count;i++){mfd_entry_view_v1 e={0};if(mfd_page_v1_entry(page,i,&e,sizeof(e))!=MFD_OK)goto done;
   if(i+1==p.entry_count){q.has_after_entry_id=1;memcpy(q.after_entry_id,e.entry_id,32);}}
  uint32_t terminal=p.terminal;if(!terminal){if(p.page_sequence==UINT64_MAX)goto done;q.page_sequence=p.page_sequence+1;}
  mfd_page_v1_free(page);page=NULL;if(terminal)break;mdr_response_free(response);response=NULL;
 }
 {mdr_snapshot s={0};if(mdr_client_snapshot(client,&s,sizeof(s))!=MDR_OK||s.phase!=MDR_TERMINAL)goto done;}
 if(mdr_response_encode(response,completion,TASK_BUFFER_BYTES,&length)!=MDR_OK)goto done;
 result=task_complete(completion,length);
done:
 mfd_page_v1_free(page);mdr_response_free(response);mdr_request_free(open);mdr_client_free(client);free(completion);free(input);return result;
}

int32_t morrow_run(void){return run(0);}
int32_t morrow_finish(void){return run(1);}
int32_t morrow_cancel(void){return run(2);}
