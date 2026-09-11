#include "morrow_plugin_sdk.h"
#include <stdlib.h>
#include <string.h>
mp_status mp_exchange(const mp_host_v1 *host, const uint8_t *request,
                      uint32_t request_length, uint8_t *output,
                      uint32_t output_capacity, uint32_t *output_length) {
  mp_host_v1 fixed_host;
  uint8_t *fixed_request;
  uint32_t received = 0, status;
  if (!output_length)
    return MP_INVALID_ARGUMENT;
  *output_length = 0;
  if (!host || !request || !output)
    return MP_INVALID_ARGUMENT;
  if (host->abi_version != MP_SDK_ABI_V1 ||
      host->struct_size < sizeof(mp_host_v1))
    return MP_ABI_MISMATCH;
  fixed_host = *host;
  if (!fixed_host.exchange || request_length == 0)
    return MP_INVALID_ARGUMENT;
  if (request_length > MP_MAX_MESSAGE_BYTES ||
      output_capacity < MP_MAX_MESSAGE_BYTES)
    return MP_LIMIT;
  fixed_request = (uint8_t *)malloc(request_length);
  if (!fixed_request)
    return MP_NO_MEMORY;
  memcpy(fixed_request, request, request_length);
  status =
      fixed_host.exchange(fixed_host.context, fixed_request, request_length,
                          output, MP_MAX_MESSAGE_BYTES, &received);
  free(fixed_request);
  if (status != 0)
    return MP_TRANSPORT_FAILURE;
  if (received == 0 || received > MP_MAX_MESSAGE_BYTES)
    return MP_BAD_REPLY;
  *output_length = received;
  return MP_OK;
}
