/* Native ABI qualification only. State replies below are fixtures, not Store proof. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <bcrypt.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "morrow_blob_upload_request_v1.h"
#include "morrow_blob_transfer_v1.h"

static unsigned checks;
#define CHECK(x) do { ++checks; if (!(x)) { fprintf(stderr,"check failed at line %u: %s\n",(unsigned)__LINE__,#x); exit(1); } } while (0)
#define OK(x) CHECK((x)==MBU_OK)
#define SIZE(x) ((uint32_t)sizeof(x))

static void hash(const uint8_t *p, uint32_t n, uint8_t out[32]) {
    BCRYPT_ALG_HANDLE a = NULL;
    BCRYPT_HASH_HANDLE h = NULL;
    CHECK(BCryptOpenAlgorithmProvider(&a,BCRYPT_SHA256_ALGORITHM,NULL,0)==0);
    CHECK(BCryptCreateHash(a,&h,NULL,0,NULL,0,0)==0);
    if (n) CHECK(BCryptHashData(h,(PUCHAR)p,n,0)==0);
    CHECK(BCryptFinishHash(h,out,32,0)==0);
    CHECK(BCryptDestroyHash(h)==0);
    CHECK(BCryptCloseAlgorithmProvider(a,0)==0);
}
static mbu_client_options options(const uint8_t *data, uint32_t length) {
    mbu_client_options o = {0};
    o.abi_version=1; o.struct_size=SIZE(o); o.max_calls=1024;
    o.max_wire_bytes=UINT64_C(67108864);
    o.descriptor.abi_version=1; o.descriptor.struct_size=SIZE(o.descriptor);
    memset(o.nomination_ref,1,32); memset(o.descriptor.transfer_epoch,2,32);
    memset(o.descriptor.object_ref,3,32); memset(o.descriptor.operation_id,4,32);
    o.descriptor.byte_length=length; hash(data,length,o.descriptor.sha256);
    return o;
}
static mbu_request_view request_view(uint32_t action, uint32_t nonce) {
    mbu_request_view v = {0};
    v.abi_version=1; v.struct_size=SIZE(v); v.action=action;
    memset(v.nomination_ref,1,32); memcpy(v.nonce,&nonce,sizeof(nonce));
    return v;
}
static mbu_reply_view state_reply(const mbu_client_options *o, uint32_t phase) {
    mbu_reply_view v = {0};
    v.abi_version=1; v.struct_size=SIZE(v); v.kind=MBU_REPLY_STATE;
    v.descriptor=o->descriptor; v.phase=phase;
    if (phase==MBU_READY || phase==MBU_RECEIVING) v.next_sequence=1;
    if (phase==MBU_STAGED || phase==MBU_RELEASED) v.received_bytes=o->descriptor.byte_length;
    return v;
}
static mbu_snapshot snapshot(mbu_client *c) {
    mbu_snapshot s={0}; OK(mbu_client_snapshot(c,&s,SIZE(s))); return s;
}
static mbu_request *begin(mbu_client *c, mbu_request_view v) {
    mbu_request *q=NULL; OK(mbu_client_begin(c,&v,SIZE(v),&q)); return q;
}
static uint32_t deliver(mbu_client *c, mbu_request *q, mbu_reply_view v) {
    mbu_response *r=NULL, *accepted=NULL;
    uint8_t wire[MBU_MAX_RESPONSE_BYTES]; uint32_t n=0;
    OK(mbu_response_new(q,&v,SIZE(v),&r));
    OK(mbu_response_encode(r,wire,SIZE(wire),&n));
    OK(mbu_client_accept(c,wire,n,&accepted));
    mbu_response_free(accepted); mbu_response_free(r); return n;
}
static void codec_boundaries(void) {
    mbu_request_view v=request_view(MBU_OPEN,1), view;
    mbu_request *q=NULL, *copy=NULL, *slot=NULL;
    uint8_t wire[MBU_MAX_REQUEST_BYTES], copywire[MBU_MAX_REQUEST_BYTES], digest[32], legacy[32];
    uint32_t n=0, m=0; union { uint32_t align; uint8_t bytes[MBU_MAX_REQUEST_BYTES]; } overlap;
    OK(mbu_request_new(&v,SIZE(v),&q));
    OK(mbu_request_view_get(q,&view,SIZE(view))); CHECK(view.action==MBU_OPEN && view.frame==NULL);
    OK(mbu_request_encode(q,wire,SIZE(wire),&n));
    OK(mbu_request_decode(wire,n,&copy));
    OK(mbu_request_encode(copy,copywire,SIZE(copywire),&m)); CHECK(m==n && memcmp(wire,copywire,n)==0);
    memset(copywire,0xa5,sizeof(copywire)); m=99;
    CHECK(mbu_request_encode(q,copywire,n-1,&m)==MBU_BUFFER); CHECK(m==99 && copywire[0]==0xa5);
    memset(&overlap,0xa5,sizeof(overlap));
    CHECK(mbu_request_encode(q,overlap.bytes,SIZE(overlap),&overlap.align)==MBU_BUFFER);
    CHECK(overlap.bytes[0]==0xa5);
    memset(&view,0xa5,sizeof(view));
    CHECK(mbu_request_view_get(q,&view,SIZE(view)-1)==MBU_BUFFER);
    CHECK(view.abi_version==UINT32_C(0xa5a5a5a5));
    slot=copy; v.reserved=1;
    CHECK(mbu_request_new(&v,SIZE(v),&slot)==MBU_INVALID); CHECK(slot==copy);
    v.reserved=0; v.abi_version=2;
    CHECK(mbu_request_new(&v,SIZE(v),&slot)==MBU_CONTRACT); CHECK(slot==copy);
    v.abi_version=1; v.frame=wire;
    CHECK(mbu_request_new(&v,SIZE(v),&slot)==MBU_INVALID); CHECK(slot==copy);
    CHECK(mbu_request_decode(NULL,1,&slot)==MBU_BUFFER); CHECK(slot==copy);
    CHECK(mbu_request_decode(NULL,MBU_MAX_REQUEST_BYTES+1,&slot)==MBU_LIMIT); CHECK(slot==copy);
    OK(mbu_schema_digest(digest,SIZE(digest))); OK(mbu_frame_schema_digest(legacy,SIZE(legacy)));
    CHECK(memcmp(digest,legacy,32)!=0); OK(mbt_schema_digest(digest,SIZE(digest)));
    CHECK(memcmp(digest,legacy,32)==0);
    mbu_request_free(copy); mbu_request_free(q); mbu_request_free(NULL); mbu_response_free(NULL); mbu_client_free(NULL);
}
static void guarded_short_inputs(void) {
    SYSTEM_INFO si; GetSystemInfo(&si);
    void *guard=VirtualAlloc(NULL,si.dwPageSize,MEM_RESERVE|MEM_COMMIT,PAGE_NOACCESS);
    mbu_request_view v=request_view(MBU_OPEN,1); mbu_request *q=NULL,*out=NULL;
    mbu_response *r=NULL; mbu_client *c=NULL;
    CHECK(guard!=NULL); OK(mbu_request_new(&v,SIZE(v),&q));
    CHECK(mbu_request_new((const mbu_request_view*)guard,0,&out)==MBU_CONTRACT); CHECK(out==NULL);
    CHECK(mbu_response_new(q,(const mbu_reply_view*)guard,0,&r)==MBU_CONTRACT); CHECK(r==NULL);
    CHECK(mbu_client_new((const mbu_client_options*)guard,0,&c)==MBU_CONTRACT); CHECK(c==NULL);
    CHECK(mbu_request_view_get(q,(mbu_request_view*)guard,0)==MBU_BUFFER);
    CHECK(mbu_schema_digest((uint8_t*)guard,31)==MBU_BUFFER);
    CHECK(VirtualFree(guard,0,MEM_RELEASE)!=0); mbu_request_free(q);
}
static int32_t failed_transport(void *context, const uint8_t *in, uint32_t n, uint8_t *out, uint32_t cap) {
    unsigned *calls=(unsigned*)context; ++*calls;
    CHECK(in!=NULL && n>0 && out!=NULL && cap==MBU_MAX_RESPONSE_BYTES); return -1;
}
static void uncertain_delivery(void) {
    mbu_client_options o=options(NULL,0); mbu_client *c=NULL;
    mbu_request_view v=request_view(MBU_OPEN,1); mbu_response *out=NULL;
    mbu_snapshot a,b; unsigned calls=0;
    OK(mbu_client_new(&o,SIZE(o),&c));
    CHECK(mbu_client_call_once(c,&v,SIZE(v),NULL,&calls,&out)==MBU_BUFFER);
    a=snapshot(c); CHECK(a.calls==0 && !a.pending);
    CHECK(mbu_client_call_once(c,&v,SIZE(v),failed_transport,&calls,&out)==MBU_STATE);
    a=snapshot(c); CHECK(calls==1 && out==NULL && a.pending && a.calls==1 && a.phase==MBU_CLIENT_UNKNOWN);
    v.nonce[0]=2;
    CHECK(mbu_client_call_once(c,&v,SIZE(v),failed_transport,&calls,&out)==MBU_STATE);
    OK(mbu_client_abandon(c)); b=snapshot(c);
    CHECK(calls==1 && b.calls==a.calls && b.wire_bytes==a.wire_bytes && b.pending && b.phase==MBU_CLIENT_UNKNOWN);
    mbu_client_free(c);
    c=NULL; OK(mbu_client_new(&o,SIZE(o),&c));
    mbu_request *q=begin(c,v); a=snapshot(c);
    /* Over-bound delivery is rejected before reading its bytes, sealing the pending owner. */
    CHECK(mbu_client_accept(c,NULL,MBU_MAX_RESPONSE_BYTES+1,&out)==MBU_LIMIT);
    b=snapshot(c); CHECK(b.phase==MBU_CLIENT_UNKNOWN && b.pending && b.calls==a.calls && b.wire_bytes==a.wire_bytes);
    CHECK(mbu_client_begin(c,&v,SIZE(v),&q)==MBU_STATE);
    mbu_request_free(q); mbu_client_free(c);
}
static void correlation_and_budgets(void) {
    mbu_client_options o=options(NULL,0); mbu_client *c=NULL;
    mbu_request_view v=request_view(MBU_OPEN,1); mbu_request *q,*other=NULL;
    mbu_reply_view rv=state_reply(&o,MBU_READY); mbu_response *r=NULL,*out=NULL;
    uint8_t bytes[MBU_MAX_RESPONSE_BYTES]; uint32_t n=0;
    OK(mbu_client_new(&o,SIZE(o),&c)); q=begin(c,v);
    v.nonce[0]=2; OK(mbu_request_new(&v,SIZE(v),&other));
    OK(mbu_response_new(other,&rv,SIZE(rv),&r)); OK(mbu_response_encode(r,bytes,SIZE(bytes),&n));
    CHECK(mbu_response_decode_for(bytes,n,q,&out)==MBU_CORRELATION); CHECK(out==NULL);
    CHECK(mbu_client_accept(c,bytes,n,&out)==MBU_CORRELATION);
    CHECK(snapshot(c).phase==MBU_CLIENT_UNKNOWN && snapshot(c).pending);
    mbu_response_free(r); mbu_request_free(q); mbu_request_free(other); mbu_client_free(c);
    c=NULL; o.max_calls=1; OK(mbu_client_new(&o,SIZE(o),&c)); q=begin(c,v);
    deliver(c,q,rv); mbu_request_free(q); q=NULL;
    v=request_view(MBU_INSPECT,3);
    CHECK(mbu_client_begin(c,&v,SIZE(v),&q)==MBU_LIMIT); CHECK(q==NULL && snapshot(c).calls==1);
    OK(mbu_client_abandon(c)); CHECK(snapshot(c).phase==MBU_CLIENT_CLOSED); mbu_client_free(c);
}
static void invalid_reply_fields(void) {
    mbu_client_options o=options(NULL,0); mbu_request_view v=request_view(MBU_OPEN,1);
    mbu_request *q=NULL; mbu_response *r=NULL; mbu_reply_view rv=state_reply(&o,MBU_READY);
    OK(mbu_request_new(&v,SIZE(v),&q));
    rv.reserved_pointer=1; CHECK(mbu_response_new(q,&rv,SIZE(rv),&r)==MBU_INVALID); CHECK(r==NULL);
    rv.reserved_pointer=0; rv.status=MBU_STATUS_DENIED;
    CHECK(mbu_response_new(q,&rv,SIZE(rv),&r)==MBU_INVALID); CHECK(r==NULL);
    rv=state_reply(&o,MBU_READY); rv.descriptor.abi_version=2;
    CHECK(mbu_response_new(q,&rv,SIZE(rv),&r)==MBU_CONTRACT); CHECK(r==NULL);
    rv=state_reply(&o,MBU_READY); rv.kind=MBU_REPLY_ERROR; rv.status=MBU_STATUS_DENIED;
    CHECK(mbu_response_new(q,&rv,SIZE(rv),&r)==MBU_INVALID); CHECK(r==NULL);
    mbu_request_free(q);
}
static uint32_t make_frame(const mbu_client_options *o, uint32_t kind, uint64_t seq, uint64_t offset,
                           const uint8_t *payload, uint32_t length, uint8_t *wire) {
    mbt_view v={0}; mbt_frame *f=NULL; uint32_t n=0;
    v.abi_version=1; v.struct_size=SIZE(v); v.kind=kind;
    memcpy(v.transfer_epoch,o->descriptor.transfer_epoch,32); memcpy(v.object_ref,o->descriptor.object_ref,32);
    memcpy(v.operation_id,o->descriptor.operation_id,32);
    if (kind==MBT_CHUNK) {
        v.sequence=seq; v.offset=offset; v.payload=payload; v.payload_length=length; hash(payload,length,v.chunk_sha256);
    } else { v.total_length=o->descriptor.byte_length; memcpy(v.whole_sha256,o->descriptor.sha256,32); }
    OK(mbt_frame_set(&v,SIZE(v),&f)); OK(mbt_frame_encode(f,wire,MBT_MAX_FRAME_BYTES,&n)); mbt_frame_free(f); return n;
}
static void transfer(uint32_t length) {
    uint8_t *data=(uint8_t*)malloc(length ? length : 1); CHECK(data!=NULL); memset(data,7,length);
    mbu_client_options o=options(data,length); mbu_client *c=NULL;
    mbt_receiver *receiver=NULL; mbt_snapshot rs={0}; mbt_frame *receipt=NULL;
    uint8_t frame[MBT_MAX_FRAME_BYTES], request_wire[MBU_MAX_REQUEST_BYTES], receipt_wire[1024];
    uint32_t nonce=1,n,rn,kind,chunks=0; uint64_t offset=0,wire_bytes=0;
    mbu_snapshot s; mbu_request *q; mbu_request_view v, borrowed;
    OK(mbu_client_new(&o,SIZE(o),&c)); OK(mbt_receiver_new(NULL,0,&receiver));
    v=request_view(MBU_OPEN,nonce++); q=begin(c,v);
    OK(mbu_request_encode(q,request_wire,SIZE(request_wire),&n)); wire_bytes+=n;
    wire_bytes+=deliver(c,q,state_reply(&o,MBU_READY)); mbu_request_free(q);
    n=make_frame(&o,MBT_DESCRIPTOR,0,0,NULL,0,frame);
    v=request_view(MBU_PUSH,nonce++); v.frame=frame; v.frame_length=n; q=begin(c,v);
    OK(mbu_request_view_get(q,&borrowed,SIZE(borrowed)));
    CHECK(borrowed.frame_length==n && memcmp(borrowed.frame,frame,n)==0);
    rn=777; CHECK(mbu_request_encode(q,(uint8_t*)borrowed.frame,MBU_MAX_REQUEST_BYTES,&rn)==MBU_BUFFER); CHECK(rn==777);
    OK(mbu_request_encode(q,request_wire,SIZE(request_wire),&rn)); wire_bytes+=rn;
    OK(mbt_receiver_accept(receiver,borrowed.frame,borrowed.frame_length,&kind,&receipt));
    CHECK(kind==MBT_ACCEPT_DESCRIPTOR && receipt==NULL);
    wire_bytes+=deliver(c,q,state_reply(&o,MBU_RECEIVING)); mbu_request_free(q);
    while (offset<length) {
        uint32_t amount=(uint32_t)((uint64_t)length-offset);
        if (amount>MBT_MAX_CHUNK_BYTES) amount=MBT_MAX_CHUNK_BYTES;
        n=make_frame(&o,MBT_CHUNK,(uint64_t)chunks+1,offset,data+(size_t)offset,amount,frame);
        CHECK(n<=MBU_MAX_EMBEDDED_FRAME_BYTES);
        v=request_view(MBU_PUSH,nonce++); v.frame=frame; v.frame_length=n; q=begin(c,v);
        OK(mbu_request_encode(q,request_wire,SIZE(request_wire),&rn)); wire_bytes+=rn;
        OK(mbu_request_view_get(q,&borrowed,SIZE(borrowed)));
        CHECK(borrowed.frame_length==n && memcmp(borrowed.frame,frame,n)==0);
        OK(mbt_receiver_accept(receiver,borrowed.frame,borrowed.frame_length,&kind,&receipt)); CHECK(kind==MBT_ACCEPT_RECEIPT);
        OK(mbt_frame_encode(receipt,receipt_wire,SIZE(receipt_wire),&rn));
        mbu_reply_view rv={0}; rv.abi_version=1; rv.struct_size=SIZE(rv); rv.kind=MBU_REPLY_RECEIPT;
        rv.receipt=receipt_wire; rv.receipt_length=rn;
        wire_bytes+=deliver(c,q,rv); mbt_frame_free(receipt); receipt=NULL; mbu_request_free(q);
        offset+=amount; ++chunks; s=snapshot(c);
        CHECK(s.phase==MBU_CLIENT_RECEIVING && !s.pending && s.received_bytes==offset && s.next_sequence==(uint64_t)chunks+1);
    }
    n=make_frame(&o,MBT_END,0,0,NULL,0,frame);
    v=request_view(MBU_PUSH,nonce++); v.frame=frame; v.frame_length=n; q=begin(c,v);
    OK(mbu_request_encode(q,request_wire,SIZE(request_wire),&rn)); wire_bytes+=rn;
    OK(mbt_receiver_accept(receiver,frame,n,&kind,&receipt)); CHECK(kind==MBT_ACCEPT_VERIFIED_BYTES && receipt==NULL);
    OK(mbt_receiver_snapshot(receiver,&rs,SIZE(rs)));
    CHECK(rs.phase==MBT_VERIFIED_BYTES && rs.received_bytes==length && memcmp(rs.verified_sha256,o.descriptor.sha256,32)==0);
    /* A fixture response exercises only client state, never durable storage. */
    wire_bytes+=deliver(c,q,state_reply(&o,MBU_STAGED)); mbu_request_free(q);
    s=snapshot(c); CHECK(s.phase==MBU_CLIENT_STAGED && s.received_bytes==length && !s.pending);
    v=request_view(MBU_RELEASE,nonce++); q=begin(c,v);
    OK(mbu_request_encode(q,request_wire,SIZE(request_wire),&rn)); wire_bytes+=rn;
    wire_bytes+=deliver(c,q,state_reply(&o,MBU_RELEASED)); mbu_request_free(q);
    s=snapshot(c); CHECK(s.phase==MBU_CLIENT_RELEASED && s.calls==chunks+4 && s.wire_bytes==wire_bytes);
    if (length==MBU_MAX_OBJECT_BYTES) CHECK(chunks==274);
    mbt_receiver_free(receiver); mbu_client_free(c); free(data);
}
int main(void) {
    codec_boundaries(); guarded_short_inputs(); uncertain_delivery(); correlation_and_budgets(); invalid_reply_fields();
    transfer(0); transfer(3); transfer((uint32_t)MBU_MAX_OBJECT_BYTES);
    printf("{\"status\":\"PASS_SCOPED_NATIVE_C\",\"scenarios\":8,\"checks\":%u,\"max_bytes\":16777216,\"max_chunks\":274,\"store_proof\":false}\n",checks);
    return 0;
}
