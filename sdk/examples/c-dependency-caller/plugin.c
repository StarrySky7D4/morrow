#include "morrow_plugin_dependency.h"
#include "morrow_plugin_task.h"
#include <stdlib.h>
#include <string.h>
static int equals(mp_span value,const char *text){size_t n=strlen(text);return value.length==n && memcmp(value.data,text,n)==0;}
int32_t morrow_run(void){
  uint8_t *input=malloc(MP_MAX_TASK_BYTES),*value=malloc(MP_MAX_TASK_VALUE_BYTES),*completion=malloc(MP_MAX_TASK_BYTES);
  mp_task *task=NULL;mp_dependency_output *output=NULL;mp_transform_view transform;
  mp_dependency_output_view result;mp_dependency_request_v1 request={0};uint32_t length=0;int32_t status=-1;
  if(!input || !value || !completion)goto done;
  int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);
  if(n<=0 || n>(int32_t)MP_MAX_TASK_BYTES || mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
  if(mp_task_get_transform(task,&transform,sizeof(transform))!=MP_CODEC_OK
    || !equals(transform.handler,"bytes.dependency-wrap") || !equals(transform.input_type,"bytes") || !equals(transform.output_type,"bytes"))goto done;
  if(transform.input.length==0 || transform.input.length>MP_MAX_TASK_VALUE_BYTES)goto done;
  for(uint32_t i=0;i<transform.input.length;i++){uint8_t b=transform.input.data[i];value[i]=(b>='a'&&b<='z')?(uint8_t)(b-32):b;}
  request.abi_version=1;request.struct_size=sizeof(request);
  request.call_id=(mp_span){(const uint8_t*)"reverse-1",9};request.slot=(mp_span){(const uint8_t*)"reverse",7};request.input=(mp_span){value,transform.input.length};
  if(mp_wasm_dependency_call(&request,&output)!=MP_CODEC_OK || mp_dependency_output_get(output,&result,sizeof(result))!=MP_CODEC_OK
    || !equals(result.output_type,"bytes") || result.bytes.length>MP_MAX_TASK_VALUE_BYTES-3)goto done;
  value[0]='A';value[1]='[';if(result.bytes.length)memcpy(value+2,result.bytes.data,result.bytes.length);value[result.bytes.length+2]=']';
  if(mp_task_output(task,value,result.bytes.length+3,completion,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
  status=mp_wasm_task_complete(completion,length);
 done:mp_dependency_output_free(output);mp_task_free(task);free(completion);free(value);free(input);return status;
}
