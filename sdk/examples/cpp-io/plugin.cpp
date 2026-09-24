// Managed IO-frame mode; the response owns the original correlated host bytes.
#include "morrow_plugin_io.hpp"
#include "morrow_plugin_task.h"
extern "C" int32_t morrow_run() {
  std::vector<uint8_t> input(MP_MAX_IO_FRAME_BYTES);
  int32_t count = mp_wasm_task_read(input.data(), MP_MAX_IO_FRAME_BYTES);
  if (count <= 0 || static_cast<uint32_t>(count) > MP_MAX_IO_FRAME_BYTES) return -1;
  input.resize(static_cast<size_t>(count));
  auto response = morrow::io_response::call_frame(input);
  if (response.status() != MP_CODEC_OK) return -1;
  auto view = response.view();
  return mp_wasm_task_complete(view.encoded_frame.data, view.encoded_frame.length);
}
