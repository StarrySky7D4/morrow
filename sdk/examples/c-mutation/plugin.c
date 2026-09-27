/* The task input is an encoded mutation Request, not a path or grant. */
#include "morrow_plugin_mutation.h"
#include "morrow_plugin_task.h"
#include <stdlib.h>

int32_t morrow_run(void) {
  uint8_t *input = (uint8_t*)malloc(MP_MAX_MUTATION_FRAME_BYTES);
  mp_mutation_response *response = NULL;
  mp_mutation_response_view view = {0};
  int32_t result = -1, count;
  if (!input) return -1;
  count = mp_wasm_task_read(input, MP_MAX_MUTATION_FRAME_BYTES);
  if (count <= 0 || (uint32_t)count > MP_MAX_MUTATION_FRAME_BYTES) goto done;
  if (mp_wasm_mutation_call_frame(input, (uint32_t)count, &response) != MP_CODEC_OK)
    goto done;
  if (mp_mutation_response_get(response, &view, sizeof(view)) != MP_CODEC_OK)
    goto done;
  result = mp_wasm_task_complete(view.encoded_frame.data, view.encoded_frame.length);
done:
  mp_mutation_response_free(response);
  free(input);
  return result;
}
