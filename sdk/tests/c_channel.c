/* Uses Rust-generated golden frames to test C protocol and alias-safe adapter. */
#include "morrow_channel_v1.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef struct fixture {uint8_t *request,*reply;uint32_t request_length,reply_length,calls,fail;} fixture;
static uint32_t exchange(void *raw,const uint8_t *input,uint32_t length,uint8_t *output,uint32_t capacity,uint32_t *written){
  fixture *f=(fixture*)raw;assert(capacity==MP_MAX_CHANNEL_WIRE_BYTES);assert(length==f->request_length);assert(memcmp(input,f->request,length)==0);f->calls++;
  memcpy(output,f->reply,f->reply_length);*written=f->reply_length;return f->fail;
}
static uint8_t* load(const char *directory,const char *name,uint32_t *length){
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
  assert(argc==2);fixture f={0};f.request=load(argv[1],"receive.request.capnp",&f.request_length);f.reply=load(argv[1],"receive.response.capnp",&f.reply_length);
  uint8_t call_id[32],reference[32],epoch[32];memset(call_id,1,32);memset(reference,2,32);memset(epoch,3,32);
  mp_channel_request_v1 request={0};request.abi_version=1;request.struct_size=sizeof(request);request.kind=MP_CHANNEL_RECEIVE;
  request.call_id.data=call_id;request.call_id.length=32;request.reference.data=reference;request.reference.length=32;request.source_epoch.data=epoch;request.source_epoch.length=32;request.credit_bytes=65536;
  mp_channel_host_v1 host={1,sizeof(host),&f,exchange};uint8_t *buffer=(uint8_t*)malloc(MP_MAX_CHANNEL_WIRE_BYTES);assert(buffer);memcpy(buffer,f.request,f.request_length);uint32_t written=99;
  assert(mp_channel_request_encode(&request,buffer,MP_MAX_CHANNEL_WIRE_BYTES,&written)==MP_CODEC_OK&&written==f.request_length&&memcmp(buffer,f.request,written)==0);
  assert(mp_channel_exchange(&host,buffer,f.request_length,buffer,MP_MAX_CHANNEL_WIRE_BYTES-1,&written)==MP_LIMIT&&written==0&&f.calls==0);
  assert(mp_channel_exchange(&host,buffer,f.request_length,buffer,MP_MAX_CHANNEL_WIRE_BYTES,&written)==MP_OK&&f.calls==1&&written==f.reply_length);
  mp_channel_response *reply=NULL;mp_channel_response_view view={0};assert(mp_channel_response_decode(buffer,written,f.request,f.request_length,&reply)==MP_CODEC_OK);assert(mp_channel_response_get(reply,&view,sizeof(view))==MP_CODEC_OK);
  assert(view.status==MP_CHANNEL_FRAME&&view.has_frame&&view.bytes.length==65536&&view.sequence==1&&view.cursor.length==4);
  uint32_t sha_length=0;uint8_t *frame_sha=load(argv[1],"frame.sha256",&sha_length);assert(sha_length==32&&view.frame_sha256.length==32&&memcmp(frame_sha,view.frame_sha256.data,32)==0);free(frame_sha);mp_channel_response_free(reply);reply=NULL;
  buffer[written]=0;assert(mp_channel_response_decode(buffer,written+1,f.request,f.request_length,&reply)!=MP_CODEC_OK&&reply==NULL);
  memcpy(buffer,f.reply,f.reply_length);buffer[16]^=1;assert(mp_channel_response_decode(buffer,f.reply_length,f.request,f.request_length,&reply)==MP_CODEC_CONTRACT&&reply==NULL);
  f.fail=1;memcpy(buffer,f.request,f.request_length);assert(mp_channel_exchange(&host,buffer,f.request_length,buffer,MP_MAX_CHANNEL_WIRE_BYTES,&written)==MP_TRANSPORT_FAILURE&&written==0&&f.calls==2);
  uint32_t directory_length=0;uint8_t *directory_bytes=load(argv[1],"directory.capnp",&directory_length);mp_channel_directory *directory=NULL;mp_channel_directory_view dv={0};
  assert(mp_channel_directory_decode(directory_bytes,directory_length,&directory)==MP_CODEC_OK);
  memset(directory_bytes,0,directory_length);assert(mp_channel_directory_get(directory,&dv,sizeof(dv))==MP_CODEC_OK);
  assert(dv.channel_count==1&&dv.scope_sha256.length==32&&dv.scope_sha256.data[0]==4&&dv.channels[0].reference.data[0]==2&&dv.channels[0].source_epoch.data[0]==3&&dv.channels[0].kind==0);
  assert(dv.channels[0].budget.max_frame_bytes==65536&&dv.channels[0].budget.max_channels==8);mp_channel_directory_free(directory);
  assert(mp_channel_directory_decode(directory_bytes,directory_length,&directory)!=MP_CODEC_OK&&directory==NULL);free(directory_bytes);
  free(buffer);free(f.reply);free(f.request);return 0;
}
