#include "morrow_plugin_task.hpp"
#include <algorithm>
#include <string_view>
static std::string_view text(mp_span s){return {reinterpret_cast<const char*>(s.data),s.length};}
extern "C" int32_t morrow_run(){
 std::vector<uint8_t> input(MP_MAX_TASK_BYTES);int32_t n=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);if(n<=0||n>(int32_t)input.size())return -1;input.resize(static_cast<size_t>(n));
 auto task=morrow::task::decode(input);if(task.status()!=MP_CODEC_OK)return -1;auto t=task.transform();if(text(t.input_type)!="bytes"||text(t.output_type)!="bytes")return -1;
 std::vector<uint8_t> value;if(t.input.length)value.assign(t.input.data,t.input.data+t.input.length);
 if(text(t.handler)=="bytes.reverse")std::reverse(value.begin(),value.end());else if(text(t.handler)=="bytes.ascii-uppercase"){for(auto& b:value)if(b>='a'&&b<='z')b=static_cast<uint8_t>(b-32);}else return -1;
 auto completion=task.output(value);if(completion.status!=MP_CODEC_OK)return -1;return mp_wasm_task_complete(completion.bytes.data(),static_cast<uint32_t>(completion.bytes.size()));
}
