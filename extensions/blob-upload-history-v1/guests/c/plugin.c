#include "../common/fixture.h"
#include "morrow_blob_upload_history_task_v1.h"
int32_t morrow_run(void) {
  uint8_t wire[MP_MAX_TASK_BYTES], output[MP_MAX_TASK_BYTES], submission[32], nonce[32];
  mp_task *task = NULL;MbhPacket *packet = NULL;
  mp_transform_view transform = {0};MbhView view = {0};
  uint32_t length = 0, status;int valid = 0, result = -1;
  int32_t n = mp_wasm_task_read(wire, sizeof wire);
  if (n <= 0 || (uint32_t)n > sizeof wire || mp_task_decode(wire, (uint32_t)n, &task) != MP_CODEC_OK) goto done;
  memset(submission, 1, sizeof submission);memset(nonce, 9, sizeof nonce);
  if (mp_task_get_transform(task, &transform, sizeof transform) == MP_CODEC_OK) {
    if (fixture_text(transform.handler.data, transform.handler.length, "history.inspect")) {
      const char *handler = "history.inspect", *type = "morrow.blob.upload.history.v1";
      valid = mbh_packet_decode_invocation(wire, (uint32_t)n, (const uint8_t*)handler, (uint32_t)strlen(handler),
        (const uint8_t*)type, (uint32_t)strlen(type), &packet) == 0 && mbh_packet_view(packet, &view, sizeof view) == 0;
    } else if (fixture_transform(&transform) &&
        mbh_packet_decode(transform.input.data, transform.input.length, submission, nonce, &packet) == 0 &&
        mbh_packet_view(packet, &view, sizeof view) == 0 && fixture_history(&view)) valid = 1;
  }
  if (valid) status = mp_task_output(task, transform.input.data, transform.input.length, output, sizeof output, &length);
  else {
    const char *message = "History fixture rejected";
    status = mp_task_fail(task, MP_TASK_INVALID_INPUT, (const uint8_t*)message, (uint32_t)strlen(message), output, sizeof output, &length);
  }
  if (status == MP_CODEC_OK && mp_wasm_task_complete(output, length) == 0) result = 0;
done:
  mbh_packet_free(packet);mp_task_free(task);return result;
}
