/* Managed IO-frame mode: no path, socket or host identity is supplied by guest. */
#include "morrow_plugin_io.h"
#include "morrow_plugin_task.h"
#include <stdlib.h>

int32_t morrow_run(void) {
  uint8_t *input = (uint8_t*)malloc(MP_MAX_IO_FRAME_BYTES);
  mp_io_response *response = NULL;
  mp_io_response_view view = {0};
  int32_t result = -1;
  if (!input) return -1;
  int32_t count = mp_wasm_task_read(input, MP_MAX_IO_FRAME_BYTES);
  if (count <= 0 || (uint32_t)count > MP_MAX_IO_FRAME_BYTES) goto done;
  if (mp_wasm_io_call_frame(input, (uint32_t)count, &response) != MP_CODEC_OK) goto done;
  if (mp_io_response_get(response, &view, sizeof(view)) != MP_CODEC_OK) goto done;
  /* A completed invocation may contain a business denial or Unknown outcome. */
  result = mp_wasm_task_complete(view.encoded_frame.data, view.encoded_frame.length);
done:
  mp_io_response_free(response);
  free(input);
  return result;
}
