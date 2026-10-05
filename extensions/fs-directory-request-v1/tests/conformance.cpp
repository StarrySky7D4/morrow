#include "morrow_fs_directory_request_v1.hpp"
#include <cassert>
#include <cstring>
#include <iostream>
#include <type_traits>
namespace sdk=morrow::fs_directory_request_v1;
static_assert(!std::is_copy_constructible_v<sdk::request>);
static_assert(!std::is_copy_constructible_v<sdk::response>);
static_assert(!std::is_copy_constructible_v<sdk::client>);
static mdr_request_view open_view(){
 mdr_request_view v{};v.abi_version=1;v.struct_size=sizeof(v);
 std::memset(v.nomination_ref,1,32);std::memset(v.nonce,2,32);return v;
}
struct peer {unsigned calls=0;bool wrong_nonce=false;};
static int32_t transport(void *opaque,const uint8_t *bytes,uint32_t length,uint8_t *output,uint32_t capacity){
 auto& p=*static_cast<peer*>(opaque);++p.calls;assert(capacity==MDR_MAX_RESPONSE_BYTES);
 sdk::request q;assert(q.decode(bytes,length)==MDR_OK);
 if(p.wrong_nonce){auto v=open_view();std::memset(v.nonce,9,32);assert(q.assign(v)==MDR_OK);}
 mdr_reply_view view{};view.abi_version=1;view.struct_size=sizeof(view);
 view.kind=MDR_REPLY_OPENED;std::memset(view.selection_epoch,3,32);view.page_sequence=1;
 sdk::response r;assert(r.assign(q,view)==MDR_OK);std::vector<uint8_t> encoded;assert(r.encode(encoded)==MDR_OK);
 assert(encoded.size()<=capacity);std::memcpy(output,encoded.data(),encoded.size());return static_cast<int32_t>(encoded.size());
}
int main(){
 sdk::request empty;std::vector<uint8_t> preserved{1,2,3};
 assert(empty.encode(preserved)==MDR_BUFFER);assert((preserved==std::vector<uint8_t>{1,2,3}));
 sdk::request original;auto v=open_view();assert(original.assign(v)==MDR_OK);
 std::vector<uint8_t> bytes;assert(original.encode(bytes)==MDR_OK);
 sdk::request moved=std::move(original);assert(original.encode(preserved)==MDR_BUFFER);
 sdk::request decoded;assert(decoded.decode(bytes.data(),static_cast<uint32_t>(bytes.size()))==MDR_OK);bytes.assign(bytes.size(),0);
 mdr_request_view got{};assert(decoded.view(got)==MDR_OK);assert(got.nomination_ref[0]==1&&got.nonce[0]==2);
 v.reserved[0]=1;assert(decoded.assign(v)==MDR_CONTRACT);
 std::vector<uint8_t> before,after;assert(moved.encode(before)==MDR_OK);assert(decoded.encode(after)==MDR_OK);assert(before==after);
 uint8_t reference[32];std::memset(reference,1,32);
 sdk::client state;assert(state.initialize(reference)==MDR_OK);
 peer p;sdk::response output;assert(state.call_once(decoded,output,transport,&p)==MDR_OK);assert(p.calls==1);
 mdr_snapshot snapshot{};assert(state.snapshot(snapshot)==MDR_OK);assert(snapshot.phase==MDR_READY);
 mdr_request_view next=open_view();next.action=MDR_NEXT;next.page_sequence=2;std::memset(next.selection_epoch,3,32);
 sdk::request syntactic;assert(syntactic.assign(next)==MDR_OK); /* syntax accepts empty-page cursor; state still expects sequence1 */
 assert(state.begin(next,syntactic)==MDR_STATE);
 sdk::client wrong;assert(wrong.initialize(reference)==MDR_OK);peer bad;bad.wrong_nonce=true;
 assert(wrong.call_once(decoded,output,transport,&bad)==MDR_CORRELATION);
 assert(wrong.snapshot(snapshot)==MDR_OK);assert(snapshot.phase==MDR_UNKNOWN&&snapshot.calls==1);
 assert(wrong.call_once(decoded,output,transport,&bad)==MDR_STATE);assert(bad.calls==1);
 std::cout<<"directory-request C++ conformance: ownership/output/correlation/single-call; no native authority proof\n";
}
