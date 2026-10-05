/* Same-source wire corpus. Checks execute in Release; no assert/NDEBUG dependency. */
#define _CRT_SECURE_NO_WARNINGS
#include "morrow_ws_message_v1.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"FAIL %s:%d %s\n",__FILE__,__LINE__,#x); exit(1); } } while(0)
typedef struct vector_record {uint32_t n,ok,enc,k,has,code,pn,wn;char *name;uint8_t *payload,*wire;} vector_record;
static uint32_t read32(FILE *f) {uint8_t b[4];CHECK(fread(b,1,4,f)==4);return (uint32_t)b[0]|((uint32_t)b[1]<<8)|((uint32_t)b[2]<<16)|((uint32_t)b[3]<<24);}
static void write32(FILE *f,uint32_t n){uint8_t b[4]={(uint8_t)n,(uint8_t)(n>>8),(uint8_t)(n>>16),(uint8_t)(n>>24)};CHECK(fwrite(b,1,4,f)==4);}
static void *alloc_bytes(size_t n){void *p=malloc(n?n:1);CHECK(p!=NULL);return p;}
static vector_record read_record(FILE *f){vector_record r;r.n=read32(f);r.ok=read32(f);r.enc=read32(f);r.k=read32(f);r.has=read32(f);r.code=read32(f);r.pn=read32(f);r.wn=read32(f);CHECK(r.n<256&&r.pn<=65536&&r.wn<=70000);r.name=(char*)alloc_bytes((size_t)r.n+1);r.payload=(uint8_t*)alloc_bytes(r.pn);r.wire=(uint8_t*)alloc_bytes(r.wn);CHECK(fread(r.name,1,r.n,f)==r.n);r.name[r.n]=0;CHECK(fread(r.payload,1,r.pn,f)==r.pn);CHECK(fread(r.wire,1,r.wn,f)==r.wn);return r;}
static void free_record(vector_record *r){free(r->name);free(r->payload);free(r->wire);}
static void write_record(FILE *f,const vector_record *r,const uint8_t *wire,uint32_t len){write32(f,r->n);write32(f,1);write32(f,1);write32(f,r->k);write32(f,r->has);write32(f,r->code);write32(f,r->pn);write32(f,len);CHECK(fwrite(r->name,1,r->n,f)==r->n);CHECK(fwrite(r->payload,1,r->pn,f)==r->pn);CHECK(fwrite(wire,1,len,f)==len);}
static void equal_message(const mws_message_v1 *m,const vector_record *r){CHECK(m->kind==r->k);CHECK(m->has_close_code==r->has);CHECK(m->close_code==r->code);CHECK(m->payload_length==r->pn);CHECK(memcmp(m->payload,r->payload,r->pn)==0);CHECK(m->reserved==0);for(size_t i=r->pn;i<65536;i++)CHECK(m->payload[i]==0);}
static int allowed_code(uint32_t c){return (c>=1000&&c<=1003)||(c>=1007&&c<=1013)||(c>=3000&&c<=4999);}
static void extra_c_methods(void){
 mws_message_v1 *m=(mws_message_v1*)alloc_bytes(sizeof(*m)),*old=(mws_message_v1*)alloc_bytes(sizeof(*m));uint8_t *buf=(uint8_t*)alloc_bytes(65536);uint32_t len=0,c;uint8_t digest[32],sentinel[32];
 memset(digest,0xa5,sizeof(digest));memcpy(sentinel,digest,sizeof(digest));CHECK(mws_message_v1_schema_digest(digest,31)!=0);CHECK(memcmp(digest,sentinel,32)==0);CHECK(mws_message_v1_schema_digest(digest,32)==0);CHECK(digest[0]==0x2d&&digest[31]==0x6d);
 memset(m,0xa5,sizeof(*m));memcpy(old,m,sizeof(*m));CHECK(mws_message_v1_set(MWS_V1_BINARY,NULL,0,0,0,m,(uint32_t)sizeof(*m))==0);CHECK(m->payload_length==0);
 memcpy(old,m,sizeof(*m));CHECK(mws_message_v1_set(MWS_V1_BINARY,NULL,1,0,0,m,(uint32_t)sizeof(*m))!=0);CHECK(memcmp(old,m,sizeof(*m))==0);CHECK(mws_message_v1_set(5,NULL,0,0,0,m,(uint32_t)sizeof(*m))!=0);CHECK(memcmp(old,m,sizeof(*m))==0);
 CHECK(mws_message_v1_set(MWS_V1_BINARY,NULL,0,2,0,m,(uint32_t)sizeof(*m))!=0);CHECK(memcmp(old,m,sizeof(*m))==0);
 CHECK(mws_message_v1_set(MWS_V1_BINARY,NULL,0,0,0,m,(uint32_t)sizeof(*m)-1)!=0);CHECK(memcmp(old,m,sizeof(*m))==0);
 printf("METHOD c_digest_null_and_abi_error_preservation PASS\n");
 for(c=0;c<=65535;c++){memcpy(old,m,sizeof(*m));uint32_t rc=mws_message_v1_set(MWS_V1_CLOSE,NULL,0,1,(uint16_t)c,m,(uint32_t)sizeof(*m));CHECK((rc==0)==allowed_code(c));if(rc!=0)CHECK(memcmp(old,m,sizeof(*m))==0);}
 printf("METHOD c_close_code_exhaustive PASS values=65536 allowed=2011\n");
 CHECK(mws_message_v1_set(MWS_V1_BINARY,(const uint8_t*)"alias",5,0,0,m,(uint32_t)sizeof(*m))==0);CHECK(mws_message_v1_set(MWS_V1_BINARY,m->payload,5,0,0,m,(uint32_t)sizeof(*m))==0);CHECK(memcmp(m->payload,"alias",5)==0);
 CHECK(mws_message_v1_encode(m,(uint32_t)sizeof(*m),buf,65536,&len)==0);memcpy(m,buf,len);CHECK(mws_message_v1_decode((const uint8_t*)m,len,m,(uint32_t)sizeof(*m))==0);CHECK(memcmp(m->payload,"alias",5)==0);
 CHECK(mws_message_v1_encode(m,(uint32_t)sizeof(*m),(uint8_t*)m,65536,&len)==0);CHECK(mws_message_v1_decode((const uint8_t*)m,len,old,(uint32_t)sizeof(*old))==0);CHECK(memcmp(old->payload,"alias",5)==0);
 printf("METHOD c_valid_input_output_aliasing PASS\n");
 CHECK(mws_message_v1_set(MWS_V1_BINARY,(const uint8_t*)"owned",5,0,0,m,(uint32_t)sizeof(*m))==0);memset(buf,0xa5,65536);uint32_t *overlap=(uint32_t*)buf;*overlap=0x12345678;CHECK(mws_message_v1_encode(m,(uint32_t)sizeof(*m),buf,65536,overlap)!=0);CHECK(*overlap==0x12345678);for(size_t j=4;j<65536;j++)CHECK(buf[j]==0xa5);
 printf("METHOD c_overlapping_output_length_refusal PASS\n");
 free(buf);free(old);free(m);
}
static void c_corpus(const char *input,const char *output){
 FILE *f=fopen(input,"rb"),*out=fopen(output,"wb");uint8_t magic[8];CHECK(f&&out);CHECK(fread(magic,1,8,f)==8);CHECK(memcmp(magic,"WSVC0002",8)==0);uint32_t count=read32(f),accepted=0,rejected=0,encoded_count=0,encode_limits=0;CHECK(fwrite(magic,1,8,out)==8);write32(out,0);
 mws_message_v1 *m=(mws_message_v1*)alloc_bytes(sizeof(*m)),*before=(mws_message_v1*)alloc_bytes(sizeof(*m)),*again=(mws_message_v1*)alloc_bytes(sizeof(*m));uint8_t *encoded=(uint8_t*)alloc_bytes(65536);
 for(uint32_t i=0;i<count;i++){
  vector_record r=read_record(f);memset(m,0xa5,sizeof(*m));memcpy(before,m,sizeof(*m));uint32_t rc=mws_message_v1_decode(r.wire,r.wn,m,(uint32_t)sizeof(*m));if((rc==0)!=(r.ok!=0)){fprintf(stderr,"VERDICT %s rc=%u expected=%u\n",r.name,rc,r.ok);exit(1);}
  if(r.ok){equal_message(m,&r);memset(r.wire,0xcc,r.wn);equal_message(m,&r);uint32_t len=0xdeadbeef;memset(encoded,0xa5,65536);CHECK(mws_message_v1_encode(m,(uint32_t)sizeof(*m),encoded,1,&len)!=0);CHECK(len==0xdeadbeef&&encoded[0]==0xa5);rc=mws_message_v1_encode(m,(uint32_t)sizeof(*m),encoded,65536,&len);if(r.enc){CHECK(rc==0);for(size_t j=len;j<65536;j++)CHECK(encoded[j]==0xa5);CHECK(mws_message_v1_decode(encoded,len,again,(uint32_t)sizeof(*again))==0);equal_message(again,&r);write_record(out,&r,encoded,len);encoded_count++;}else{CHECK(rc==MWS_V1_LIMIT&&len==0xdeadbeef);for(size_t j=0;j<65536;j++)CHECK(encoded[j]==0xa5);encode_limits++;}CHECK(mws_message_v1_set(r.k,r.payload,r.pn,r.has,(uint16_t)r.code,again,(uint32_t)sizeof(*again))==0);memset(r.payload,0xcc,r.pn);CHECK(memcmp(m->payload,again->payload,r.pn)==0);accepted++;}
  else{CHECK(memcmp(m,before,sizeof(*m))==0);rejected++;}
  printf("VECTOR %s %s\n",r.name,r.ok?"ACCEPT":"REJECT");free_record(&r);
 }
 CHECK(fgetc(f)==EOF);CHECK(fseek(out,8,SEEK_SET)==0);write32(out,encoded_count);CHECK(fclose(f)==0);CHECK(fclose(out)==0);free(encoded);free(again);free(before);free(m);
 printf("METHOD c_corpus_verdict_and_fields PASS vectors=%u accept=%u reject=%u\n",count,accepted,rejected);
 printf("METHOD c_decode_and_set_input_ownership PASS\n");printf("METHOD c_encode_error_outputs_unchanged PASS\n");printf("METHOD c_owned_semantic_roundtrips PASS\n");
 printf("METHOD c_decode_max_wire_encode_limit PASS encode_limits=%u serialized_roundtrips=%u\n",encode_limits,encoded_count);
}
#ifndef MWS_CONFORMANCE_CPP
int main(int argc,char **argv){CHECK(argc==3);c_corpus(argv[1],argv[2]);extra_c_methods();return 0;}
#endif
