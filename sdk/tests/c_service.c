/* Native codec check. stdin/stdout carry core-produced request/reply frames. */
#include "morrow_plugin_service.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(void) {
  uint8_t *input=malloc(MP_MAX_SERVICE_FRAME_BYTES), *output=malloc(MP_MAX_SERVICE_FRAME_BYTES);
  mp_service_request *request=NULL;
  mp_service_request_view view={0};
  mp_service_reply_v1 reply={0};
  uint32_t length=91;
  assert(input && output);
  size_t count=fread(input,1,MP_MAX_SERVICE_FRAME_BYTES,stdin);
  assert(count>0 && !ferror(stdin));
  assert(mp_service_request_decode(NULL,1,&request)==MP_CODEC_INVALID && request==NULL);
  assert(mp_service_request_decode(input,(uint32_t)count,&request)==MP_CODEC_OK);
  memset(input,0,count); free(input); /* handle must own original request bytes */
  assert(mp_service_request_get(request,&view,sizeof(view))==MP_CODEC_OK);
  assert(view.call_id==UINT64_MAX && view.header_count==2);
  assert(view.body.length==3 && view.body.data[0]==0 && view.body.data[1]==255);
  assert(view.principal.length==5 && memcmp(view.principal.data,"alice",5)==0);
  reply.abi_version=MP_SERVICE_ABI_VERSION; reply.struct_size=sizeof(reply);
  reply.status=200; reply.body=view.body; reply.headers=view.headers; reply.header_count=view.header_count;
  memset(output,0xaa,MP_MAX_SERVICE_FRAME_BYTES);
  assert(mp_service_response_encode(request,&reply,output,1,&length)==MP_CODEC_LIMIT && length==0 && output[0]==0xaa);
  reply.status=65536+200;
  assert(mp_service_response_encode(request,&reply,output,MP_MAX_SERVICE_FRAME_BYTES,&length)==MP_CODEC_INVALID && length==0);
  reply.status=204;
  assert(mp_service_response_encode(request,&reply,output,MP_MAX_SERVICE_FRAME_BYTES,&length)==MP_CODEC_INVALID && length==0);
  reply.status=200;
  assert(mp_service_response_encode(request,&reply,output,MP_MAX_SERVICE_FRAME_BYTES,&length)==MP_CODEC_OK);
  mp_service_request_free(request); mp_service_request_free(NULL);
  assert(fwrite(output,1,length,stdout)==length); free(output);
  return 0;
}
