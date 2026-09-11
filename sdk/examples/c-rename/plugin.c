#include "morrow_plugin_codec.h"
#include "morrow_plugin_wasm.h"
#include <stdlib.h>
int32_t morrow_run(void) {
  mp_request_v1 request = {0};
  request.abi_version = 1;
  request.struct_size = sizeof(request);
  request.kind = MP_REQUEST_RENAME;
  request.request_id = (mp_span){(const uint8_t *)"wasm-op", 7};
  request.card_id = (mp_span){(const uint8_t *)"legacy-123", 10};
  request.title = (mp_span){(const uint8_t *)"Wasm SDK rename", 15};
  request.revision = 1;
  uint8_t *input = (uint8_t *)malloc(MP_MAX_MESSAGE_BYTES),
          *output = (uint8_t *)malloc(MP_MAX_MESSAGE_BYTES);
  if (!input || !output) {
    free(input);
    free(output);
    return -1;
  }
  uint32_t length = 0, written = 0;
  int32_t result = 99;
  mp_reply *reply = 0;
  mp_reply_view view;
  mp_host_v1 host = mp_wasm_host();
  if (mp_request_encode(&request, input, MP_MAX_MESSAGE_BYTES, &length) !=
      MP_CODEC_OK)
    goto done;
  if (mp_exchange(&host, input, length, output, MP_MAX_MESSAGE_BYTES,
                  &written) != MP_OK) {
    result = -1;
    goto done;
  }
  if (mp_reply_decode(output, written, &request, &reply) != MP_CODEC_OK)
    goto done;
  if (mp_reply_get(reply, &view, sizeof(view)) != MP_CODEC_OK)
    goto done;
  if (view.kind == MP_REPLY_REJECTED && view.failure == 0)
    result = 10;
  if (view.kind == MP_REPLY_RENAMED && view.revision == 2)
    result = 20;
done:
  mp_reply_free(reply);
  free(output);
  free(input);
  return result;
}
