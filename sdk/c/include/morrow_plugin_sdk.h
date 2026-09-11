#ifndef MORROW_PLUGIN_SDK_H
#define MORROW_PLUGIN_SDK_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Experimental transport adapter, not a plugin loader or security boundary.
 * Payload is precompiled Cap'n Proto. No storage path, self-selected identity,
 * grant management or direct core handle API is exposed to guest code.
 * Native pointers are local to an adapter; they are NEVER the IPC/Wasm wire
 * ABI.
 */
#define MP_SDK_ABI_V1 1u
#define MP_MAX_MESSAGE_BYTES 65536u
typedef uint32_t mp_status;
#define MP_OK 0u
#define MP_INVALID_ARGUMENT 1u
#define MP_ABI_MISMATCH 2u
#define MP_LIMIT 3u
#define MP_NO_MEMORY 4u
#define MP_TRANSPORT_FAILURE 5u
#define MP_BAD_REPLY 6u
/* The adapter validates its own channel/instance; context is not a capability.
 * The callback must not retain buffers, exceed output_capacity, throw across C,
 * or re-enter/concurrently use this session. Nonzero means transport failure;
 * it says nothing about whether an operation has committed. No automatic retry.
 */
typedef uint32_t (*mp_exchange_fn)(void *context, const uint8_t *request,
                                   uint32_t request_length, uint8_t *output,
                                   uint32_t output_capacity,
                                   uint32_t *output_length);
typedef struct mp_host_v1 {
  uint32_t abi_version;
  uint32_t struct_size;
  void *context;
  mp_exchange_fn exchange;
} mp_host_v1;
/* Valid caller-owned pointers and exclusive access are required for this call.
 * Reserve a full-size output BEFORE submission; undersized output never calls
 * host. Request is copied before the callback. Input/output may overlap. Only
 * MP_OK exposes a nonzero output_length. Error output bytes are not a reply.
 * MP_OK means a reply arrived, NOT business success; decode and correlate it.
 */
mp_status mp_exchange(const mp_host_v1 *host, const uint8_t *request,
                      uint32_t request_length, uint8_t *output,
                      uint32_t output_capacity, uint32_t *output_length);
#ifdef __cplusplus
}
#endif
#endif
