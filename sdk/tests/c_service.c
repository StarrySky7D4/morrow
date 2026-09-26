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
  assert(view.call_id==UINT64_MAX && view.header_count>=2 && view.header_count<=4);
  assert(view.body.length==3 && view.body.data[0]==0 && view.body.data[1]==255);
  assert(view.principal.length==5 && memcmp(view.principal.data,"alice",5)==0);
  uint8_t digest[32];memset(digest,0xa5,sizeof(digest));
  assert(mp_service_request_digest(request,digest,31)==MP_CODEC_LIMIT && digest[0]==0xa5);
  assert(mp_service_request_digest(NULL,digest,32)==MP_CODEC_INVALID && digest[0]==0xa5);
  assert(mp_service_request_digest(request,digest,32)==MP_CODEC_OK);
  const char *expected=getenv("MORROW_SDK_EXPECTED_SERVICE_DIGEST"), *hex="0123456789abcdef";
  assert(expected && strlen(expected)==64);
  for(uint32_t i=0;i<32;i++){assert(expected[2*i]==hex[digest[i]>>4]);assert(expected[2*i+1]==hex[digest[i]&15]);}
  mp_service_resources *resources=NULL;
  uint32_t resources_status=mp_service_request_resources(request,&resources);
  assert(resources_status==(view.header_count==4 ? MP_CODEC_INVALID : MP_CODEC_OK));
  assert((resources!=NULL)==(view.header_count==3));
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
  if(resources) {
    mp_service_resources_view rv={0};
    assert(mp_service_resources_get(resources,&rv,sizeof(rv)-1)==MP_CODEC_LIMIT);
    assert(mp_service_resources_get(resources,&rv,sizeof(rv))==MP_CODEC_OK);
    assert(rv.scope_sha256.length==32 && rv.scope_sha256.data[0]==7 && rv.endpoint_count==1);
    const mp_service_endpoint *e=&rv.endpoints[0];
    assert(e->reference.length==64 && e->reference.data[0]=='a');
    assert(e->credential.length==16 && memcmp(e->credential.data,"opaque-reference",16)==0);
    assert(e->method_count==2 && e->methods[1].length==4 && memcmp(e->methods[1].data,"POST",4)==0);
    assert(e->max_request_bytes==1024 && e->max_response_bytes==2048 && e->timeout_ms==3000 && e->response_frame_limit==4096);
    mp_service_resources_free(resources);
  }
  mp_service_resources_free(NULL);
  assert(fwrite(output,1,length,stdout)==length); free(output);
  return 0;
}
