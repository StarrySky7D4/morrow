#include "morrow_plugin_dependency.hpp"
#include "morrow_plugin_task.hpp"
/* Compile-only explicit C++ dependency transport; no declaration can grant host permissions. */
extern "C" int morrow_run(){
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES);int32_t length=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
  if(length<=0 || static_cast<uint32_t>(length)>MP_MAX_TASK_BYTES)return 1;
  input.resize(static_cast<uint32_t>(length));auto task=morrow::task::decode(input);
  if(task.status()!=MP_CODEC_OK)return 2;
  morrow::dependency_request request("call1","reverse",{'a','b','c'});
  auto output=morrow::dependency_output::call(request);if(output.status()!=MP_CODEC_OK)return 3;
  auto view=output.view();std::vector<uint8_t> bytes(view.bytes.data,view.bytes.data+view.bytes.length);
  auto completion=task.output(bytes);if(completion.status!=MP_CODEC_OK)return 4;
  return mp_wasm_task_complete(completion.bytes.data(),static_cast<uint32_t>(completion.bytes.size()))==0?0:5;
}
