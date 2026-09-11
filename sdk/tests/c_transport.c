#include "morrow_plugin_sdk.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
struct state {
  uint32_t calls, status, length;
  uint8_t *original;
};
static uint32_t exchange(void *context, const uint8_t *input, uint32_t length,
                         uint8_t *output, uint32_t cap, uint32_t *written) {
  struct state *s = (struct state *)context;
  ++s->calls;
  assert(cap == MP_MAX_MESSAGE_BYTES);
  if (s->original)
    s->original[0] = 42;
  memcpy(output, input, length);
  *written = s->length ? s->length : length;
  return s->status;
}
int main(void) {
  struct state state = {0, 0, 0, NULL};
  mp_host_v1 host = {1, sizeof(mp_host_v1), &state, exchange};
  uint8_t input[] = {0, 255, 9};
  uint8_t *output = (uint8_t *)malloc(MP_MAX_MESSAGE_BYTES);
  uint32_t length = 9;
  assert(output);
  assert(mp_exchange(&host, input, 3, output, 1, &length) == MP_LIMIT &&
         length == 0 && state.calls == 0);
  state.original = input;
  assert(mp_exchange(&host, input, 3, output, MP_MAX_MESSAGE_BYTES, &length) ==
             MP_OK &&
         length == 3);
  assert(input[0] == 42 && output[0] == 0 && output[1] == 255 &&
         state.calls == 1);
  state.original = NULL;
  state.status = 77;
  assert(mp_exchange(&host, input, 3, output, MP_MAX_MESSAGE_BYTES, &length) ==
             MP_TRANSPORT_FAILURE &&
         length == 0 && state.calls == 2);
  state.status = 0;
  state.length = MP_MAX_MESSAGE_BYTES + 1;
  assert(mp_exchange(&host, input, 3, output, MP_MAX_MESSAGE_BYTES, &length) ==
             MP_BAD_REPLY &&
         length == 0);
  state.length = 0;
  host.abi_version = 2;
  assert(mp_exchange(&host, input, 3, output, MP_MAX_MESSAGE_BYTES, &length) ==
         MP_ABI_MISMATCH);
  host.abi_version = 1;
  host.exchange = NULL;
  assert(mp_exchange(&host, input, 3, output, MP_MAX_MESSAGE_BYTES, &length) ==
         MP_INVALID_ARGUMENT);
  free(output);
  puts("PASS: C SDK immutable input, preflight, bounds, no retry and ABI "
       "rejection");
  return 0;
}
