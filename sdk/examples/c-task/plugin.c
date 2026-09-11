#include "morrow_plugin_task.h"
#include "morrow_plugin_wasm.h"
#include <stdlib.h>
int32_t morrow_run(void) {
 uint8_t *input=malloc(MP_MAX_TASK_BYTES),*response=malloc(MP_MAX_MESSAGE_BYTES),*output=malloc(MP_MAX_TASK_BYTES);
 mp_task* task=0;mp_task_view view;uint32_t size=0,length=0;int32_t result=-1;
 if(!input||!response||!output)goto done;
 int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);
 if(n<=0||n>(int32_t)MP_MAX_TASK_BYTES||mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
 if(mp_task_get(task,&view,sizeof(view))!=MP_CODEC_OK)goto done;
 mp_host_v1 host=mp_wasm_host();
 if(mp_exchange(&host,view.command.data,view.command.length,response,MP_MAX_MESSAGE_BYTES,&size)!=MP_OK)goto done;
 if(mp_task_complete(task,response,size,output,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
 result=mp_wasm_task_complete(output,length);
done:
 mp_task_free(task);free(output);free(response);free(input);return result;
}
