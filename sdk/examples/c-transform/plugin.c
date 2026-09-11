#include "morrow_plugin_task.h"
#include <stdlib.h>
#include <string.h>
static int equals(mp_span value,const char* text){size_t n=strlen(text);return value.length==n&&memcmp(value.data,text,n)==0;}
int32_t morrow_run(void){
 uint8_t *input=malloc(MP_MAX_TASK_BYTES),*value=malloc(MP_MAX_TASK_VALUE_BYTES),*completion=malloc(MP_MAX_TASK_BYTES);mp_task* task=0;mp_transform_view t;uint32_t length=0;int32_t result=-1;
 if(!input||!value||!completion)goto done;
 int32_t n=mp_wasm_task_read(input,MP_MAX_TASK_BYTES);if(n<=0||n>(int32_t)MP_MAX_TASK_BYTES||mp_task_decode(input,(uint32_t)n,&task)!=MP_CODEC_OK)goto done;
 if(mp_task_get_transform(task,&t,sizeof(t))!=MP_CODEC_OK||!equals(t.input_type,"bytes")||!equals(t.output_type,"bytes"))goto done;
 if(equals(t.handler,"bytes.reverse")){for(uint32_t i=0;i<t.input.length;i++)value[i]=t.input.data[t.input.length-1-i];}
 else if(equals(t.handler,"bytes.ascii-uppercase")){for(uint32_t i=0;i<t.input.length;i++){uint8_t b=t.input.data[i];value[i]=(b>='a'&&b<='z')?(uint8_t)(b-32):b;}}
 else if(equals(t.handler,"bytes.require-ascii")){
   for(uint32_t i=0;i<t.input.length;i++){
     if(t.input.data[i]>127){
       const char* message="Input contains non-ASCII bytes";
       if(mp_task_fail(task,MP_TASK_UNSUPPORTED_INPUT,(const uint8_t*)message,(uint32_t)strlen(message),completion,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
       result=mp_wasm_task_complete(completion,length);goto done;
     }
     value[i]=t.input.data[i];
   }
 }
 else goto done;
 if(mp_task_output(task,value,t.input.length,completion,MP_MAX_TASK_BYTES,&length)!=MP_CODEC_OK)goto done;
 result=mp_wasm_task_complete(completion,length);
done:mp_task_free(task);free(completion);free(value);free(input);return result;
}
