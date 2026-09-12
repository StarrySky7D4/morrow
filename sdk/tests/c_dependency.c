#include "morrow_plugin_dependency.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static mp_span text(const char *s){mp_span v={(const uint8_t*)s,(uint32_t)strlen(s)};return v;}
int main(int argc,char **argv){
  uint8_t *request=(uint8_t*)malloc(MP_MAX_DEPENDENCY_BYTES),*response=(uint8_t*)malloc(MP_MAX_DEPENDENCY_BYTES),digest[32];
  uint32_t length=0;size_t size;FILE *file;mp_dependency_output *output=NULL;mp_dependency_output_view view;
  mp_dependency_request_v1 raw={0};assert(argc==2 && request && response);
  raw.abi_version=1;raw.struct_size=sizeof(raw);raw.call_id=text("call1");raw.slot=text("reverse");raw.input=text("abc");
  assert(mp_dependency_schema_digest(digest,sizeof(digest))==MP_CODEC_OK);
  assert(mp_dependency_request_encode(&raw,request,MP_MAX_DEPENDENCY_BYTES,&length)==MP_CODEC_OK && length>0);
  file=fopen(argv[1],"rb");assert(file);size=fread(response,1,MP_MAX_DEPENDENCY_BYTES,file);assert(!ferror(file));assert(fgetc(file)==EOF);fclose(file);
  assert(size>0 && mp_dependency_response_decode(response,(uint32_t)size,request,length,&output)==MP_CODEC_OK);
  memset(response,0,size);assert(mp_dependency_output_get(output,&view,sizeof(view))==MP_CODEC_OK);
  assert(view.output_type.length==5 && memcmp(view.output_type.data,"bytes",5)==0);
  assert(view.bytes.length==3 && memcmp(view.bytes.data,"def",3)==0);mp_dependency_output_free(output);
  memset(request,0xab,MP_MAX_DEPENDENCY_BYTES);assert(mp_dependency_request_encode(&raw,request,1,&length)==MP_CODEC_LIMIT && length==0 && request[0]==0xab);
  raw.input.length=0;assert(mp_dependency_request_encode(&raw,request,MP_MAX_DEPENDENCY_BYTES,&length)==MP_CODEC_INVALID && length==0);
  free(request);free(response);puts("PASS_SCOPED C dependency codec ownership and bounded failures");return 0;
}
