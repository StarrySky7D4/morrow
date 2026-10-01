#include "morrow_channel_v1.h"
#include <stdlib.h>
#include <string.h>
uint32_t mp_channel_exchange(const mp_channel_host_v1 *host,const uint8_t *request,uint32_t length,
    uint8_t *output,uint32_t capacity,uint32_t *written) {
  mp_channel_host_v1 fixed;uint8_t *input=NULL,*reply=NULL;uint32_t status,n=0;
  if(!written)return MP_INVALID_ARGUMENT;
  *written=0;
  if(!host||!request||!output||!length)return MP_INVALID_ARGUMENT;
  {
    uint32_t header[2];
    if((uintptr_t)host % _Alignof(mp_channel_host_v1))return MP_ABI_MISMATCH;
    memcpy(header,host,sizeof(header));
    if(header[0]!=MP_CHANNEL_VERSION||header[1]<sizeof(fixed))return MP_ABI_MISMATCH;
  }
  memcpy(&fixed,host,sizeof(fixed));
  if(!fixed.call)return MP_INVALID_ARGUMENT;
  if(length>MP_MAX_CHANNEL_WIRE_BYTES||capacity<MP_MAX_CHANNEL_WIRE_BYTES)return MP_LIMIT;
  input=(uint8_t*)malloc(MP_MAX_CHANNEL_WIRE_BYTES);
  reply=(uint8_t*)malloc(MP_MAX_CHANNEL_WIRE_BYTES);
  if(!input||!reply){free(input);free(reply);return MP_NO_MEMORY;}
  memcpy(input,request,length);
  status=mp_channel_request_validate(input,length);
  if(status==MP_CODEC_OK){
    status=fixed.call(fixed.context,input,length,reply,MP_MAX_CHANNEL_WIRE_BYTES,&n);
    if(status!=0)status=MP_TRANSPORT_FAILURE;
    else if(!n||n>MP_MAX_CHANNEL_WIRE_BYTES)status=MP_BAD_REPLY;
    else {memmove(output,reply,n);*written=n;}
  }
  free(reply);free(input);return status;
}
uint32_t mp_channel_call(const mp_channel_host_v1 *host,const mp_channel_request_v1 *request,mp_channel_response **out){
  uint8_t *input=NULL,*reply=NULL;uint32_t status,n=0,length=0;
  if(!out)return MP_CODEC_INVALID;
  *out=NULL;
  input=(uint8_t*)malloc(MP_MAX_CHANNEL_WIRE_BYTES);reply=(uint8_t*)malloc(MP_MAX_CHANNEL_WIRE_BYTES);
  if(!input||!reply){free(input);free(reply);return MP_NO_MEMORY;}
  status=mp_channel_request_encode(request,input,MP_MAX_CHANNEL_WIRE_BYTES,&length);
  if(status==MP_CODEC_OK)status=mp_channel_exchange(host,input,length,reply,MP_MAX_CHANNEL_WIRE_BYTES,&n);
  if(status==MP_CODEC_OK)status=mp_channel_response_decode(reply,n,input,length,out);
  free(reply);free(input);return status;
}
