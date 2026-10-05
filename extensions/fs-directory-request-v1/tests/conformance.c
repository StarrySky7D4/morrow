#include "morrow_fs_directory_request_v1.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

typedef struct peer_context { unsigned calls; int fail; } peer_context;
static mdr_request_view request_view(uint32_t action,unsigned nonce) {
 mdr_request_view v={0};v.abi_version=1;v.struct_size=sizeof(v);v.action=action;
 memset(v.nomination_ref,1,32);memset(v.nonce,(int)nonce,32);
 if(action!=MDR_OPEN)memset(v.selection_epoch,3,32);
 return v;
}
static int32_t one_transport(void *opaque,const uint8_t *input,uint32_t length,uint8_t *output,uint32_t capacity) {
 peer_context *context=(peer_context*)opaque;context->calls++;
 assert(capacity==MDR_MAX_RESPONSE_BYTES);if(context->fail)return 0;
 mdr_request *q=NULL;mdr_response *r=NULL;mdr_request_view qv={0};mdr_reply_view rv={0};
 assert(mdr_request_decode(input,length,&q)==MDR_OK);
 assert(mdr_request_get_view(q,&qv,sizeof(qv))==MDR_OK);
 rv.abi_version=1;rv.struct_size=sizeof(rv);
 if(qv.action==MDR_OPEN){rv.kind=MDR_REPLY_OPENED;memset(rv.selection_epoch,3,32);rv.page_sequence=1;rv.entries=1;rv.metadata_bytes=64;}
 else if(qv.action==MDR_FINISH)rv.kind=MDR_REPLY_FINISHED;
 else if(qv.action==MDR_CANCEL)rv.kind=MDR_REPLY_CANCELLED;
 else {rv.kind=MDR_REPLY_ERROR;rv.status=MDR_STATUS_DENIED;}
 assert(mdr_response_set(q,&rv,sizeof(rv),&r)==MDR_OK);
 uint32_t written=0;assert(mdr_response_encode(r,output,capacity,&written)==MDR_OK);
 mdr_response_free(r);mdr_request_free(q);return (int32_t)written;
}
int main(void) {
 for(unsigned action=MDR_OPEN;action<=MDR_CANCEL;action++){
  mdr_request_view v=request_view(action,2);if(action==MDR_NEXT)v.page_sequence=2; /* empty prior page permits no after-ID */
  mdr_request *q=NULL,*decoded=NULL;assert(mdr_request_set(&v,sizeof(v),&q)==MDR_OK);
  uint8_t raw[512];memset(raw,0xa5,sizeof(raw));uint32_t length=77;
  assert(mdr_request_encode(q,raw,1,&length)==MDR_BUFFER);assert(length==77);
  for(unsigned i=0;i<sizeof(raw);i++)assert(raw[i]==0xa5);
  assert(mdr_request_encode(q,raw,sizeof(raw),&length)==MDR_OK);
  assert(mdr_request_decode(raw,length,&decoded)==MDR_OK);memset(raw,0,sizeof(raw));
  mdr_request_view got={0};assert(mdr_request_get_view(decoded,&got,sizeof(got))==MDR_OK);
  assert(got.action==action&&got.nomination_ref[0]==1&&got.nonce[0]==2);
  mdr_request *retained=decoded;assert(mdr_request_decode(NULL,1,&decoded)==MDR_BUFFER);assert(decoded==retained);
  v.reserved[0]=1;assert(mdr_request_set(&v,sizeof(v),&decoded)==MDR_CONTRACT);assert(decoded==retained);
  mdr_request_free(decoded);mdr_request_free(q);
 }
 for(unsigned cancel=0;cancel<2;cancel++){
  uint8_t ref[32];memset(ref,1,32);mdr_client *client=NULL;
  assert(mdr_client_new(ref,32,&client)==MDR_OK);
  mdr_request_view open=request_view(MDR_OPEN,2);mdr_request *q=NULL;assert(mdr_request_set(&open,sizeof(open),&q)==MDR_OK);
  peer_context peer={0,0};mdr_response *r=NULL;
  assert(mdr_client_call_request_once(client,q,one_transport,&peer,&r)==MDR_OK);
  mdr_response_view view={0};assert(mdr_response_get_view(r,&view,sizeof(view))==MDR_OK);
  assert(view.reply.kind==MDR_REPLY_OPENED&&view.reply.selection_epoch[0]==3);
  mdr_response_free(r);r=NULL;mdr_request_free(q);
  mdr_request_view end=request_view(cancel?MDR_CANCEL:MDR_FINISH,5);
  assert(mdr_client_call_once(client,&end,sizeof(end),one_transport,&peer,&r)==MDR_OK);
  mdr_snapshot snapshot={0};assert(mdr_client_snapshot(client,&snapshot,sizeof(snapshot))==MDR_OK);
  assert(snapshot.phase==MDR_TERMINAL&&snapshot.calls==2&&snapshot.resident_nonces==0);
  assert(mdr_client_call_once(client,&open,sizeof(open),one_transport,&peer,&r)==MDR_STATE);
  assert(peer.calls==2);mdr_response_free(r);mdr_client_free(client);
 }
 uint8_t ref[32];memset(ref,1,32);mdr_client *client=NULL;mdr_response *r=NULL;
 assert(mdr_client_new(ref,32,&client)==MDR_OK);peer_context lost={0,1};
 mdr_request_view open=request_view(MDR_OPEN,2);
 assert(mdr_client_call_once(client,&open,sizeof(open),one_transport,&lost,&r)==MDR_OUTCOME_UNKNOWN);
 assert(r==NULL);mdr_snapshot snapshot={0};assert(mdr_client_snapshot(client,&snapshot,sizeof(snapshot))==MDR_OK);
 assert(snapshot.phase==MDR_UNKNOWN&&snapshot.calls==1&&snapshot.request_wire_bytes>0);
 assert(mdr_client_call_once(client,&open,sizeof(open),one_transport,&lost,&r)==MDR_STATE);
 assert(lost.calls==1);mdr_client_free(client);
 puts("directory-request C conformance: actions4 terminal2 loss1; no native authority/join proof");
 return 0;
}
