#include "morrow_plugin_task.hpp"
#include "morrow_plugin_sdk.hpp"
#include "morrow_plugin_wasm.h"
extern "C" int32_t morrow_run() {
 std::vector<uint8_t> input(MP_MAX_TASK_BYTES);int32_t length=mp_wasm_task_read(input.data(),MP_MAX_TASK_BYTES);
 if(length<=0||length>(int32_t)input.size())return -1;input.resize(static_cast<size_t>(length));
 auto task=morrow::task::decode(input);if(task.status()!=MP_CODEC_OK)return -1;
 auto view=task.view();morrow::client client(mp_wasm_host());auto response=client.exchange(view.command.data,view.command.length);
 if(response.status!=MP_OK)return -1;auto completion=task.complete(response.bytes);if(completion.status!=MP_CODEC_OK)return -1;
 return mp_wasm_task_complete(completion.bytes.data(),static_cast<uint32_t>(completion.bytes.size()));
}
