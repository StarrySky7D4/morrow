#define _CRT_SECURE_NO_WARNINGS
#include "morrow_sse_event_v1.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#ifdef __cplusplus
static_assert(sizeof(mse_event_v1)==65576,"SSE C ABI struct size");
#else
_Static_assert(sizeof(mse_event_v1)==65576,"SSE C ABI struct size");
#endif
#define CHECK(x) do{if(!(x)){fprintf(stderr,"FAIL %s:%d %s\n",__FILE__,__LINE__,#x);exit(1);}}while(0)
typedef struct record {uint32_t n,ok,enc,has,dn,en,in,wn;uint64_t retry;char *name;uint8_t *d,*e,*id,*w;}record;
static void *allocate(size_t n){void *p=malloc(n?n:1);CHECK(p!=NULL);return p;}
static uint32_t read32(FILE *f){uint8_t b[4];CHECK(fread(b,1,4,f)==4);return (uint32_t)b[0]|((uint32_t)b[1]<<8)|((uint32_t)b[2]<<16)|((uint32_t)b[3]<<24);}
static void write32(FILE *f,uint32_t v){uint8_t b[4]={(uint8_t)v,(uint8_t)(v>>8),(uint8_t)(v>>16),(uint8_t)(v>>24)};CHECK(fwrite(b,1,4,f)==4);}
static record read_record(FILE *f){record r;r.n=read32(f);r.ok=read32(f);r.enc=read32(f);r.has=read32(f);r.retry=read32(f);r.retry|=(uint64_t)read32(f)<<32;r.dn=read32(f);r.en=read32(f);r.in=read32(f);r.wn=read32(f);CHECK(r.n<256&&r.dn<=65536&&r.en<=65536&&r.in<=65536&&r.wn<=70000);r.name=(char*)allocate((size_t)r.n+1);r.d=(uint8_t*)allocate(r.dn);r.e=(uint8_t*)allocate(r.en);r.id=(uint8_t*)allocate(r.in);r.w=(uint8_t*)allocate(r.wn);CHECK(fread(r.name,1,r.n,f)==r.n);r.name[r.n]=0;CHECK(fread(r.d,1,r.dn,f)==r.dn);CHECK(fread(r.e,1,r.en,f)==r.en);CHECK(fread(r.id,1,r.in,f)==r.in);CHECK(fread(r.w,1,r.wn,f)==r.wn);return r;}
static void release_record(record *r){free(r->name);free(r->d);free(r->e);free(r->id);free(r->w);}
static void write_record(FILE *f,const record *r,const uint8_t *w,uint32_t wn){write32(f,r->n);write32(f,1);write32(f,1);write32(f,r->has);write32(f,(uint32_t)r->retry);write32(f,(uint32_t)(r->retry>>32));write32(f,r->dn);write32(f,r->en);write32(f,r->in);write32(f,wn);CHECK(fwrite(r->name,1,r->n,f)==r->n);CHECK(fwrite(r->d,1,r->dn,f)==r->dn);CHECK(fwrite(r->e,1,r->en,f)==r->en);CHECK(fwrite(r->id,1,r->in,f)==r->in);CHECK(fwrite(w,1,wn,f)==wn);}
static void equal_event(const mse_event_v1 *v,const record *r){CHECK(v->data_offset==0&&v->data_length==r->dn);CHECK(v->event_offset==r->dn&&v->event_length==r->en);CHECK(v->id_offset==r->dn+r->en&&v->id_length==r->in);CHECK(v->has_retry==r->has&&v->retry==r->retry&&v->reserved==0);CHECK(memcmp(v->storage,r->d,r->dn)==0);CHECK(memcmp(v->storage+r->dn,r->e,r->en)==0);CHECK(memcmp(v->storage+r->dn+r->en,r->id,r->in)==0);for(size_t j=(size_t)r->dn+r->en+r->in;j<65536;j++)CHECK(v->storage[j]==0);}
static void unchanged_buffer(const uint8_t *b,uint32_t n){for(uint32_t j=0;j<n;j++)CHECK(b[j]==0xa5);}
static void c_extra_methods(void){
 mse_event_v1 *v=(mse_event_v1*)allocate(sizeof(*v)),*before=(mse_event_v1*)allocate(sizeof(*v)),*copy=(mse_event_v1*)allocate(sizeof(*v));uint8_t *buf=(uint8_t*)allocate(65536),*normal=(uint8_t*)allocate(65537);uint32_t size=(uint32_t)sizeof(*v),length;memset(normal,'x',65537);
 memset(v,0xa5,sizeof(*v));memcpy(before,v,sizeof(*v));uint8_t digest[32],digest_before[32];memset(digest,0xa5,32);memcpy(digest_before,digest,32);CHECK(mse_event_v1_schema_digest(digest,31)==MSE_V1_BUFFER);CHECK(memcmp(digest,digest_before,32)==0);CHECK(mse_event_v1_schema_digest(digest,32)==0);CHECK(digest[0]==0xbe&&digest[31]==0x63);
 CHECK(mse_event_v1_set(NULL,0,NULL,0,NULL,0,0,0,v,size)==0);CHECK(v->data_length==0&&v->event_length==0&&v->id_length==0);memcpy(before,v,sizeof(*v));CHECK(mse_event_v1_set(NULL,1,NULL,0,NULL,0,0,0,v,size)==MSE_V1_INVALID);CHECK(memcmp(v,before,sizeof(*v))==0);CHECK(mse_event_v1_set(NULL,0,NULL,0,NULL,0,0,0,v,size-1)==MSE_V1_INVALID);CHECK(memcmp(v,before,sizeof(*v))==0);
 printf("METHOD c_sse_digest_null_and_abi_output_preservation PASS\n");
 memcpy(before,v,sizeof(*v));CHECK(mse_event_v1_set(normal,32768,normal+32768,1,normal+32769,32768,1,UINT64_MAX,v,size)==MSE_V1_LIMIT);CHECK(memcmp(v,before,sizeof(*v))==0);CHECK(mse_event_v1_set(normal,65537,NULL,0,NULL,0,0,0,v,size)==MSE_V1_LIMIT);CHECK(memcmp(v,before,sizeof(*v))==0);
 CHECK(mse_event_v1_set(normal,65536,NULL,0,NULL,0,1,UINT64_MAX,v,size)==0);CHECK(v->data_length==65536);memset(buf,0xa5,65536);length=0xdeadbeef;CHECK(mse_event_v1_encode(v,size,buf,65536,&length)==MSE_V1_LIMIT);CHECK(length==0xdeadbeef);unchanged_buffer(buf,65536);
 printf("METHOD c_sse_borrowed_aggregate_limit_before_owned_output PASS values=3\n");
 CHECK(mse_event_v1_set((const uint8_t*)"x",1,NULL,0,NULL,0,0,0,v,size)==0);memcpy(before,v,sizeof(*v));
 for(uint32_t mode=0;mode<7;mode++){
  memcpy(v,before,sizeof(*v));if(mode==0)v->reserved=1;else if(mode==1)v->has_retry=2;else if(mode==2)v->retry=1;else if(mode==3){v->data_offset=65535;v->data_length=2;}else if(mode==4){v->data_offset=65537;v->data_length=0;}else if(mode==5){v->data_length=UINT32_MAX;}else{v->data_offset=0;v->data_length=32768;v->event_offset=0;v->event_length=1;v->id_offset=32768;v->id_length=32768;}
  memcpy(copy,v,sizeof(*v));memset(buf,0xa5,65536);length=0xdeadbeef;uint32_t rc=mse_event_v1_encode(v,size,buf,65536,&length);CHECK(rc==(mode>=5?MSE_V1_LIMIT:MSE_V1_INVALID));CHECK(memcmp(copy,v,sizeof(*v))==0);CHECK(length==0xdeadbeef);unchanged_buffer(buf,65536);
 }
 printf("METHOD c_sse_offsets_reserved_retry_and_owned_aggregate_errors PASS values=7\n");
 CHECK(mse_event_v1_set((const uint8_t*)"same",4,(const uint8_t*)"same",4,(const uint8_t*)"same",4,1,UINT64_MAX,v,size)==0);v->event_offset=0;v->id_offset=0;CHECK(mse_event_v1_encode(v,size,buf,65536,&length)==0);CHECK(mse_event_v1_decode(buf,length,copy,size)==0);CHECK(copy->data_length==4&&copy->event_length==4&&copy->id_length==4&&copy->retry==UINT64_MAX);CHECK(memcmp(copy->storage,"samesamesame",12)==0);
 CHECK(mse_event_v1_set(v->storage,4,v->storage,4,v->storage,4,1,UINT64_MAX,v,size)==0);CHECK(memcmp(v->storage,"samesamesame",12)==0);CHECK(mse_event_v1_encode(v,size,buf,65536,&length)==0);memcpy(v,buf,length);CHECK(mse_event_v1_decode((const uint8_t*)v,length,v,size)==0);CHECK(memcmp(v->storage,"samesamesame",12)==0);CHECK(mse_event_v1_encode(v,size,(uint8_t*)v,65536,&length)==0);CHECK(mse_event_v1_decode((const uint8_t*)v,length,copy,size)==0);CHECK(memcmp(copy->storage,"samesamesame",12)==0);
 CHECK(mse_event_v1_set(NULL,0,NULL,0,NULL,0,0,0,v,size)==0);v->data_offset=v->event_offset=v->id_offset=65536;CHECK(mse_event_v1_encode(v,size,buf,65536,&length)==0);CHECK(mse_event_v1_decode(buf,length,copy,size)==0);CHECK(copy->data_length==0&&copy->event_length==0&&copy->id_length==0);
 printf("METHOD c_sse_overlapping_field_ranges_and_input_output_aliases PASS\n");
 CHECK(mse_event_v1_set((const uint8_t*)"x",1,NULL,0,NULL,0,0,0,v,size)==0);memset(buf,0xa5,65536);uint32_t *overlap=(uint32_t*)buf;*overlap=0x12345678;CHECK(mse_event_v1_encode(v,size,buf,65536,overlap)==MSE_V1_INVALID);CHECK(*overlap==0x12345678);unchanged_buffer(buf+4,65532);
 printf("METHOD c_sse_encode_output_length_overlap_refusal PASS\n");free(normal);free(buf);free(copy);free(before);free(v);
}
static void c_corpus(const char *input,const char *output){
 FILE *f=fopen(input,"rb"),*out=fopen(output,"wb");CHECK(f&&out);uint8_t magic[8];CHECK(fread(magic,1,8,f)==8);CHECK(memcmp(magic,"SEVC0001",8)==0);CHECK(fwrite(magic,1,8,out)==8);write32(out,0);uint32_t count=read32(f),accept=0,reject=0,encoded_count=0,limits=0;
 mse_event_v1 *v=(mse_event_v1*)allocate(sizeof(*v)),*before=(mse_event_v1*)allocate(sizeof(*v)),*again=(mse_event_v1*)allocate(sizeof(*v));uint8_t *encoded=(uint8_t*)allocate(65536);uint32_t size=(uint32_t)sizeof(*v);
 for(uint32_t idx=0;idx<count;idx++){
  record r=read_record(f);memset(v,0xa5,sizeof(*v));memcpy(before,v,sizeof(*v));uint32_t rc=mse_event_v1_decode(r.w,r.wn,v,size);if((rc==0)!=(r.ok!=0)){fprintf(stderr,"VERDICT %s rc=%u expected=%u\n",r.name,rc,r.ok);exit(1);}
  if(r.ok){equal_event(v,&r);memset(r.w,0xcc,r.wn);equal_event(v,&r);memset(encoded,0xa5,65536);uint32_t len=0xdeadbeef;CHECK(mse_event_v1_encode(v,size,encoded,1,&len)==(r.enc?MSE_V1_BUFFER:MSE_V1_LIMIT));CHECK(len==0xdeadbeef);unchanged_buffer(encoded,65536);rc=mse_event_v1_encode(v,size,encoded,65536,&len);if(r.enc){CHECK(rc==0);unchanged_buffer(encoded+len,65536-len);CHECK(mse_event_v1_decode(encoded,len,again,size)==0);equal_event(again,&r);write_record(out,&r,encoded,len);encoded_count++;}else{CHECK(rc==MSE_V1_LIMIT&&len==0xdeadbeef);unchanged_buffer(encoded,65536);limits++;}CHECK(mse_event_v1_set(r.d,r.dn,r.e,r.en,r.id,r.in,r.has,r.retry,again,size)==0);equal_event(again,&r);memset(r.d,0xcc,r.dn);memset(r.e,0xcc,r.en);memset(r.id,0xcc,r.in);CHECK(memcmp(v,again,sizeof(*v))==0);accept++;}else{CHECK(memcmp(v,before,sizeof(*v))==0);reject++;}
  printf("VECTOR %s %s\n",r.name,r.ok?"ACCEPT":"REJECT");release_record(&r);
 }
 CHECK(fgetc(f)==EOF);CHECK(fseek(out,8,SEEK_SET)==0);write32(out,encoded_count);CHECK(fclose(f)==0);CHECK(fclose(out)==0);free(encoded);free(again);free(before);free(v);
 printf("METHOD c_sse_corpus_verdict_complete_fields_and_zero_tail PASS vectors=%u accept=%u reject=%u\n",count,accept,reject);printf("METHOD c_sse_decode_set_own_all_inputs PASS\n");printf("METHOD c_sse_encode_error_and_unused_capacity_preservation PASS\n");printf("METHOD c_sse_roundtrips_and_independent_encode_bound PASS encoded=%u encode_limits=%u\n",encoded_count,limits);
}
#ifndef MSE_CONFORMANCE_CPP
int main(int argc,char **argv){CHECK(argc==3);c_corpus(argv[1],argv[2]);c_extra_methods();return 0;}
#endif
