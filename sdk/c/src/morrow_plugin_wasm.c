#include "morrow_plugin_wasm.h"
#if !defined(__wasm32__)
#error "This adapter requires wasm32"
#endif
__attribute__((import_module("morrow_v1"),
               import_name("exchange"))) extern int32_t
morrow_wasm_exchange(const uint8_t *, uint32_t, uint8_t *, uint32_t);
static uint32_t adapter(void *context, const uint8_t *input, uint32_t length,
                        uint8_t *output, uint32_t capacity, uint32_t *written) {
  (void)context;
  int32_t result = morrow_wasm_exchange(input, length, output, capacity);
  if (result <= 0)
    return 1;
  *written = (uint32_t)result;
  return 0;
}
mp_host_v1 mp_wasm_host(void) {
  mp_host_v1 host = {MP_SDK_ABI_V1, sizeof(mp_host_v1), 0, adapter};
  return host;
}
