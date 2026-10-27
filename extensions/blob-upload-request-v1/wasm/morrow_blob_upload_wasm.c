#include "morrow_blob_upload_wasm.h"
#if !defined(__wasm32__)
#error "This adapter is only for Wasm guests"
#endif
__attribute__((import_module("morrow_blob_upload_v1"), import_name("call")))
extern int32_t upload_call(const uint8_t *, uint32_t, uint8_t *, uint32_t);
__attribute__((import_module("morrow_task_v1"), import_name("complete")))
extern int32_t upload_complete(const uint8_t *, uint32_t);
int32_t mbu_wasm_transport(void *context, const uint8_t *input, uint32_t length,
                           uint8_t *output, uint32_t capacity) {
    if (context || !input || !output || !length || length > MBU_MAX_REQUEST_BYTES ||
        capacity != MBU_MAX_RESPONSE_BYTES) return -1;
    return upload_call(input, length, output, capacity);
}
uint32_t mbu_wasm_complete_response(const mbu_response *response) {
    uint8_t bytes[MBU_MAX_RESPONSE_BYTES];
    uint32_t length = 0;
    uint32_t code = mbu_response_encode(response, bytes, sizeof(bytes), &length);
    if (code != MBU_OK) return code;
    return upload_complete(bytes, length) == 0 ? MBU_OK : MBU_STATE;
}
