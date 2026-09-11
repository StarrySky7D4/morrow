#include "morrow_plugin_task.h"
#if !defined(__wasm32__)
#error This import adapter is only for wasm32 guests
#endif
__attribute__((import_module("morrow_task_v1"),import_name("read_input")))
extern int32_t task_input(uint8_t*,uint32_t);
__attribute__((import_module("morrow_task_v1"),import_name("complete")))
extern int32_t task_complete(const uint8_t*,uint32_t);
int32_t mp_wasm_task_read(uint8_t* output,uint32_t capacity){return task_input(output,capacity);}
int32_t mp_wasm_task_complete(const uint8_t* input,uint32_t length){return task_complete(input,length);}
