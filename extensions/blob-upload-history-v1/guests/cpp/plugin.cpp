#include "../common/fixture.h"
#include "morrow_blob_upload_history_v1.hpp"
#include "morrow_blob_upload_history_task_v1.hpp"
#include "morrow_plugin_task.hpp"
#include <array>
#include <vector>
extern "C" int32_t morrow_run() {
  std::vector<uint8_t> wire(MP_MAX_TASK_BYTES);
  int32_t n = mp_wasm_task_read(wire.data(), static_cast<uint32_t>(wire.size()));
  if (n <= 0 || static_cast<uint32_t>(n) > wire.size()) return -1;
  wire.resize(static_cast<size_t>(n));auto task = morrow::task::decode(wire);
  if (task.status() != MP_CODEC_OK) return -1;
  auto input = task.transform();std::array<uint8_t,32> submission{}, nonce{};
  submission.fill(1);nonce.fill(9);
  std::vector<uint8_t> bytes(input.input.data, input.input.data + input.input.length);
  MbhView view{};bool valid;
  if (fixture_text(input.handler.data, input.handler.length, "history.inspect")) {
    morrow::upload_history_v1::TaskPacket original;
    valid = original.decode(wire, "history.inspect", "morrow.blob.upload.history.v1") == 0;
    morrow::upload_history_v1::TaskPacket moved(std::move(original));
    valid = valid && original.view(view) == 5 && moved.view(view) == 0;
  } else {
    morrow::upload_history_v1::Packet original;
    valid = fixture_transform(&input) && original.decode(bytes, submission, nonce) == 0;
    morrow::upload_history_v1::Packet moved(std::move(original));
    valid = valid && original.view(view) == 5 && moved.view(view) == 0 && fixture_history(&view);
  }
  auto output = valid ? task.output(bytes) : task.fail(MP_TASK_INVALID_INPUT, "History fixture rejected");
  if (output.status != MP_CODEC_OK) return -1;
  return mp_wasm_task_complete(output.bytes.data(), static_cast<uint32_t>(output.bytes.size())) == 0 ? 0 : -1;
}
