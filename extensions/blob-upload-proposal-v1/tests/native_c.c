#include "morrow_blob_upload_proposal_v1.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>
static const uint8_t empty_sha[32] = {0xe3,0xb0,0xc4,0x42,0x98,0xfc,0x1c,0x14,0x9a,0xfb,0xf4,0xc8,0x99,0x6f,0xb9,0x24,0x27,0xae,0x41,0xe4,0x64,0x9b,0x93,0x4c,0xa4,0x95,0x99,0x1b,0x78,0x52,0xb8,0x55};
static mbp_proposal_view proposal_view(uint64_t length) {
    mbp_proposal_view view = {0};
    static const uint8_t label[] = "Native sample \xf0\x9f\x98\x80";
    view.abi_version=MBP_VERSION;view.struct_size=sizeof(view);view.byte_length=length;
    memset(view.whole_sha256,9,32);if(!length) memcpy(view.whole_sha256,empty_sha,32);
    view.label=label;view.label_length=sizeof(label)-1;return view;
}
static mbp_identity identity(void) {
    mbp_identity value={0};value.abi_version=MBP_VERSION;value.struct_size=sizeof(value);
    memset(value.transfer_epoch,1,32);memset(value.object_ref,2,32);memset(value.operation_id,3,32);return value;
}
static void round_trip(uint64_t length,uint32_t input_length) {
    uint8_t input[MBP_MAX_PRODUCER_INPUT_BYTES],wire[MBP_MAX_BOUND_INPUT_BYTES];
    memset(input,42,sizeof(input));mbp_proposal_view value=proposal_view(length);
    value.producer_input=input;value.producer_input_length=input_length;
    mbp_proposal *proposal=NULL,*decoded=NULL;mbp_bound_input *bound=NULL,*reopened=NULL;
    mbp_identity id=identity(),actual={0};uint32_t size=0;
    assert(mbp_proposal_new(&value,sizeof(value),&proposal)==MBP_OK);
    assert(mbp_proposal_encode(proposal,wire,sizeof(wire),&size)==MBP_OK);
    assert(mbp_proposal_decode(wire,size,&decoded)==MBP_OK);
    assert(mbp_bound_new(decoded,&id,sizeof(id),&bound)==MBP_OK);
    mbp_proposal_free(proposal);mbp_proposal_free(decoded);memset(input,0,sizeof(input));
    assert(mbp_bound_proposal_view(bound,&value,sizeof(value))==MBP_OK);
    assert(value.byte_length==length && value.producer_input_length==input_length);
    for(uint32_t i=0;i<input_length;++i) assert(value.producer_input[i]==42);
    assert(mbp_bound_identity(bound,&actual,sizeof(actual))==MBP_OK);
    assert(memcmp(actual.operation_id,id.operation_id,32)==0);
    memset(wire,0xab,sizeof(wire));assert(mbp_bound_encode(bound,wire,sizeof(wire),&size)==MBP_OK);
    for(uint32_t i=size;i<sizeof(wire);++i)assert(wire[i]==0xab);
    assert(mbp_bound_decode(wire,size,&reopened)==MBP_OK);
    mbp_bound_free(reopened);mbp_bound_free(bound);
}
static void failures_preserve_outputs(void) {
    mbp_proposal_view value=proposal_view(1);mbp_proposal *p=NULL;
    assert(mbp_proposal_new(&value,sizeof(value),&p)==MBP_OK);
    uint8_t wire[MBP_MAX_PROPOSAL_BYTES];memset(wire,0xab,sizeof(wire));uint32_t written=99;
    assert(mbp_proposal_encode(p,wire,1,&written)==MBP_BUFFER);assert(written==99);
    for(unsigned i=0;i<sizeof(wire);++i)assert(wire[i]==0xab);
    mbp_proposal *slot=p;value.reserved=1;
    assert(mbp_proposal_new(&value,sizeof(value),&slot)==MBP_INVALID && slot==p);
    value.reserved=0;value.struct_size--;
    assert(mbp_proposal_new(&value,sizeof(value),&slot)==MBP_CONTRACT && slot==p);
    assert(mbp_proposal_decode(wire,1,&slot)!=MBP_OK && slot==p);
    assert(mbp_proposal_encode(p,wire,sizeof(wire),&written)==MBP_OK);wire[16]^=1;
    assert(mbp_proposal_decode(wire,written,&slot)!=MBP_OK && slot==p);mbp_proposal_free(p);
}
static void borrowed_views_and_guarded_ranges(void) {
    mbp_proposal_view value=proposal_view(1);mbp_proposal *p=NULL;
    assert(mbp_proposal_new(&value,sizeof(value),&p)==MBP_OK);
    assert(mbp_proposal_view_get(p,&value,sizeof(value))==MBP_OK);uint32_t written=99;
    assert(mbp_proposal_encode(p,(uint8_t*)value.label,MBP_MAX_PROPOSAL_BYTES,&written)==MBP_BUFFER);
    assert(written==99);assert(mbp_proposal_view_get(p,&value,1)==MBP_BUFFER);
    mbp_identity id=identity();mbp_bound_input *bound=NULL;
    assert(mbp_bound_new(p,&id,sizeof(id),&bound)==MBP_OK);
    assert(mbp_bound_proposal_view(bound,(mbp_proposal_view*)bound,sizeof(value))==MBP_BUFFER);
    assert(mbp_bound_identity(bound,(mbp_identity*)bound,sizeof(id))==MBP_BUFFER);
    mbp_bound_free(bound);mbp_proposal_free(p);mbp_proposal_free(NULL);mbp_bound_free(NULL);
}
static void semantic_limits(void) {
    mbp_proposal_view v=proposal_view(1);mbp_proposal *p=NULL;
    v.byte_length=(uint64_t)MBP_MAX_OBJECT_BYTES+1;assert(mbp_proposal_new(&v,sizeof(v),&p)==MBP_LIMIT && p==NULL);
    v=proposal_view(0);v.whole_sha256[0]^=1;assert(mbp_proposal_new(&v,sizeof(v),&p)==MBP_INVALID && p==NULL);
    v=proposal_view(1);v.label=(const uint8_t*)"bad\nlabel";v.label_length=9;assert(mbp_proposal_new(&v,sizeof(v),&p)==MBP_INVALID && p==NULL);
}
int main(void) {
    round_trip(0,0);round_trip(150017,7);round_trip(MBP_MAX_OBJECT_BYTES,MBP_MAX_PRODUCER_INPUT_BYTES);
    failures_preserve_outputs();borrowed_views_and_guarded_ranges();semantic_limits();
    puts("{\"status\":\"PASS_SCOPED_NATIVE_C\",\"scenarios\":6,\"store_proof\":false}");return 0;
}
