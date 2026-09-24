#include "morrow_plugin_io.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static mp_span span(const void *data,uint32_t length){mp_span v={(const uint8_t*)data,length};return v;}
static uint32_t read_file(const char *path,uint8_t *out){
  FILE *file=NULL;size_t length;
#if defined(_WIN32)
  assert(fopen_s(&file,path,"rb")==0);
#else
  file=fopen(path,"rb");
#endif
  assert(file);
  length=fread(out,1,MP_MAX_IO_FRAME_BYTES,file);assert(!ferror(file) && fgetc(file)==EOF);fclose(file);
  assert(length>0 && length<=MP_MAX_IO_FRAME_BYTES);return (uint32_t)length;
}
int main(int argc,char **argv){
  uint8_t *request=(uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  uint8_t *original=(uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  uint8_t *response=(uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  uint8_t reference[32],digest[32];uint32_t length=99,request_length,response_length;
  mp_io_request_v1 raw={0};mp_io_response *owned=NULL;mp_io_response_view view={0};
  assert(argc==3 && request && original && response);memset(reference,7,sizeof(reference));
  raw.abi_version=MP_IO_ABI_VERSION;raw.struct_size=sizeof(raw);raw.kind=MP_IO_FILE_READ;
  raw.call_id=77;raw.reference=span(reference,32);raw.operation_id=span("operation-1",11);raw.deadline_ms=1000;
  assert(mp_io_schema_digest(digest,31)==MP_CODEC_LIMIT);
  assert(mp_io_schema_digest(digest,32)==MP_CODEC_OK);
  request_length=read_file(argv[1],original);response_length=read_file(argv[2],response);
  assert(mp_io_request_encode(&raw,request,MP_MAX_IO_FRAME_BYTES,&length)==MP_CODEC_OK);
  assert(length==request_length && memcmp(request,original,length)==0);
  assert(mp_io_response_decode(response,response_length,request,length,&owned)==MP_CODEC_OK && owned);
  memset(response,0,response_length);
  assert(mp_io_response_get(owned,&view,sizeof(view))==MP_CODEC_OK);
  assert(view.status==MP_IO_STATUS_OUTCOME_UNKNOWN && view.bytes.length==0);
  assert(view.encoded_frame.length==response_length && view.encoded_frame.data);
  mp_io_response_free(owned);owned=NULL;
  raw.call_id=78;
  assert(mp_io_request_encode(&raw,request,MP_MAX_IO_FRAME_BYTES,&length)==MP_CODEC_OK);
  response_length=read_file(argv[2],response);
  assert(mp_io_response_decode(response,response_length,request,length,&owned)==MP_CODEC_CORRELATION && !owned);
  assert(mp_io_response_get(owned,&view,sizeof(view))==MP_CODEC_INVALID);
  assert(view.encoded_frame.data==NULL && view.encoded_frame.length==0);
  memset(request,0xa5,MP_MAX_IO_FRAME_BYTES);raw.reference.length=31;length=99;
  assert(mp_io_request_encode(&raw,request,MP_MAX_IO_FRAME_BYTES,&length)==MP_CODEC_INVALID);
  assert(length==0 && request[0]==0xa5);
  raw.reference.length=32;
  assert(mp_io_request_encode(&raw,request,1,&length)==MP_CODEC_LIMIT);
  assert(length==0 && request[0]==0xa5);
  free(request);free(original);free(response);
  puts("PASS_SCOPED C IO codec original-frame correlation, owned view and bounds");return 0;
}
