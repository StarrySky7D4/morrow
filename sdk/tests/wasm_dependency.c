#include "morrow_plugin_dependency.h"
#include "morrow_plugin_task.h"
#include <stdlib.h>
/* Compile-only explicit dependency transport probe; not a host authorization test. */
int morrow_run(void){
  uint8_t *buffer=(uint8_t*)malloc(MP_MAX_TASK_BYTES);mp_task *task=NULL;
  mp_dependency_output *output=NULL;mp_dependency_output_view view;
  mp_dependency_request_v1 request={0};uint32_t written=0,status;int32_t length;
  if(!buffer)return 1;
  length=mp_wasm_task_read(buffer,MP_MAX_TASK_BYTES);
  if(length<=0 || (uint32_t)length>MP_MAX_TASK_BYTES){free(buffer);return 2;}
  status=mp_task_decode(buffer,(uint32_t)length,&task);
  request.abi_version=1;request.struct_size=sizeof(request);
  request.call_id=(mp_span){(const uint8_t*)"call1",5};
  request.slot=(mp_span){(const uint8_t*)"reverse",7};
  request.input=(mp_span){(const uint8_t*)"abc",3};
  if(status==MP_CODEC_OK)status=mp_wasm_dependency_call(&request,&output);
  if(status==MP_CODEC_OK)status=mp_dependency_output_get(output,&view,sizeof(view));
  if(status==MP_CODEC_OK)status=mp_task_output(task,view.bytes.data,view.bytes.length,buffer,MP_MAX_TASK_BYTES,&written);
  if(status==MP_CODEC_OK && mp_wasm_task_complete(buffer,written)!=0)status=MP_TRANSPORT_FAILURE;
  mp_dependency_output_free(output);mp_task_free(task);free(buffer);return status==MP_CODEC_OK?0:3;
}
