/* Real PAGE_NOACCESS boundary; no heap padding can hide a full-struct read. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include "morrow_channel_v1.h"
#include "morrow_plugin_dependency.h"
#include "morrow_plugin_io.h"
#include "morrow_plugin_mutation.h"
#include "morrow_plugin_service.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static uint8_t *load(const char *directory,const char *name,uint32_t *length){
  char path[4096];int n=snprintf(path,sizeof(path),"%s/%s",directory,name);assert(n>0&&(size_t)n<sizeof(path));
  FILE *file=NULL;
#if defined(_WIN32)
  assert(fopen_s(&file,path,"rb")==0);
#else
  file=fopen(path,"rb");
#endif
  assert(file);assert(fseek(file,0,SEEK_END)==0);long size=ftell(file);assert(size>0);
  size_t byte_count=(size_t)size;assert(byte_count<=MP_MAX_CHANNEL_WIRE_BYTES);rewind(file);
  uint8_t *bytes=(uint8_t*)malloc(byte_count);assert(bytes);assert(fread(bytes,1,byte_count,file)==byte_count);fclose(file);*length=(uint32_t)byte_count;return bytes;
}
int main(int argc,char **argv){
  SYSTEM_INFO info;GetSystemInfo(&info);size_t page=info.dwPageSize;assert(page>=4096);
  uint8_t *pages=(uint8_t*)VirtualAlloc(NULL,page*2,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);assert(pages);DWORD old=0;assert(VirtualProtect(pages+page,page,PAGE_NOACCESS,&old));
  uint8_t *prefix=pages+page-8,*output=(uint8_t*)malloc(131072);assert(output);uint32_t written=99;memset(output,0xa5,131072);
  assert(argc==2);uint32_t service_length=0;uint8_t *service_bytes=load(argv[1],"service.request.capnp",&service_length);mp_service_request *service=NULL;
  assert(mp_service_request_decode(service_bytes,service_length,&service)==MP_CODEC_OK);
  uint32_t headers[3][2]={{1,8},{0,UINT32_MAX},{1,0}};
  for(uint32_t test=0;test<3;test++){
    memcpy(prefix,headers[test],8);
#define CHECK_ENCODE(function,type) do{written=99;assert(function((const type*)prefix,output,131072,&written)==MP_CODEC_CONTRACT);assert(written==0&&output[0]==0xa5&&output[131071]==0xa5);}while(0)
    CHECK_ENCODE(mp_channel_request_encode,mp_channel_request_v1);
    CHECK_ENCODE(mp_request_encode,mp_request_v1);
    CHECK_ENCODE(mp_content_request_encode,mp_content_request_v1);
    CHECK_ENCODE(mp_dependency_request_encode,mp_dependency_request_v1);
    CHECK_ENCODE(mp_io_request_encode,mp_io_request_v1);
    CHECK_ENCODE(mp_mutation_request_encode,mp_mutation_request_v1);
    written=99;assert(mp_service_response_encode(service,(const mp_service_reply_v1*)prefix,output,131072,&written)==MP_CODEC_CONTRACT&&written==0);
    written=99;assert(mp_channel_exchange((const mp_channel_host_v1*)prefix,(const uint8_t*)"x",1,output,131072,&written)==MP_ABI_MISMATCH&&written==0);
    written=99;assert(mp_exchange((const mp_host_v1*)prefix,(const uint8_t*)"x",1,output,131072,&written)==MP_ABI_MISMATCH&&written==0);
  }
  mp_service_request_free(service);free(service_bytes);free(output);assert(VirtualFree(pages,0,MEM_RELEASE));
#if defined(MP_GUARD_CPP)
  puts("C++ Windows guardpage: 27 rejection cases passed");
#else
  puts("C Windows guardpage: 27 rejection cases passed");
#endif
  return 0;
}
