#include "morrow_channel_v1.h"
#include <assert.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
_Static_assert(sizeof(void *) == 8, "This qualification is Windows x64 only");
_Static_assert(sizeof(mp_channel_host_v1) == 24, "Rust HostV1 ABI size");
_Static_assert(offsetof(mp_channel_host_v1, context) == 8, "Rust HostV1 context offset");
_Static_assert(offsetof(mp_channel_host_v1, call) == 16, "Rust HostV1 callback offset");
static uint32_t fail(void *context, const uint8_t *input, uint32_t length,
                     uint8_t *output, uint32_t capacity, uint32_t *written) {
  uint32_t *calls = (uint32_t *)context;
  assert(mp_channel_request_validate(input, length) == MP_CODEC_OK);
  assert(capacity == MP_MAX_CHANNEL_WIRE_BYTES);
  (*calls)++; memset(output, 0xa5, capacity); *written = 0; return 9;
}
int main(void) {
  uint8_t call[32], reference[32], epoch[32], wire[MP_MAX_CHANNEL_WIRE_BYTES];
  memset(call, 1, sizeof(call)); memset(reference, 2, sizeof(reference));
  memset(epoch, 3, sizeof(epoch));
  mp_channel_request_v1 request = {0};
  request.abi_version = 1; request.struct_size = sizeof(request); request.kind = MP_CHANNEL_QUERY;
  request.call_id.data = call; request.call_id.length = 32;
  request.reference.data = reference; request.reference.length = 32;
  request.source_epoch.data = epoch; request.source_epoch.length = 32;
  uint32_t length = 0, calls = 0;
  assert(mp_channel_request_encode(&request, wire, sizeof(wire), &length) == MP_CODEC_OK);
  assert(mp_channel_request_validate(wire, length) == MP_CODEC_OK);
  mp_channel_host_v1 host = {1, sizeof(host), &calls, fail};
  mp_channel_response *response = NULL;
  assert(mp_channel_call(&host, &request, &response) == MP_TRANSPORT_FAILURE);
  assert(response == NULL && calls == 1);
  printf("PASS standalone C11 DLL/wrapper: host=%zu context=%zu call=%zu request=%zu\n",
         sizeof(host), offsetof(mp_channel_host_v1, context), offsetof(mp_channel_host_v1, call), sizeof(request));
  return 0;
}
