#include "morrow_plugin_dependency.hpp"
#include "morrow_plugin_task.hpp"
#include <string_view>
static std::string_view text(mp_span value){return {reinterpret_cast<const char*>(value.data),value.length};}
extern "C" int32_t morrow_run(){
  std::vector<uint8_t> input(MP_MAX_TASK_BYTES);int32_t n=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
  if(n<=0 || n>(int32_t)input.size())return -1;input.resize(static_cast<size_t>(n));
  auto task=morrow::task::decode(input);if(task.status()!=MP_CODEC_OK)return -1;auto t=task.transform();
  if(text(t.handler)!="bytes.dependency-wrap" || text(t.input_type)!="bytes" || text(t.output_type)!="bytes" || t.input.length==0)return -1;
  std::vector<uint8_t> value(t.input.data,t.input.data+t.input.length);for(auto& b:value)if(b>='a'&&b<='z')b=static_cast<uint8_t>(b-32);
  morrow::dependency_request request("reverse-1","reverse",std::move(value));auto output=morrow::dependency_output::call(request);
  if(output.status()!=MP_CODEC_OK)return -1;auto result=output.view();if(text(result.output_type)!="bytes" || result.bytes.length>MP_MAX_TASK_VALUE_BYTES-3)return -1;
  value={'A','['};if(result.bytes.length)value.insert(value.end(),result.bytes.data,result.bytes.data+result.bytes.length);value.push_back(']');
  auto completion=task.output(value);if(completion.status!=MP_CODEC_OK)return -1;
  return mp_wasm_task_complete(completion.bytes.data(),static_cast<uint32_t>(completion.bytes.size()));
}
