#ifndef MORROW_BLOB_UPLOAD_WASM_H
#define MORROW_BLOB_UPLOAD_WASM_H
#include "morrow_blob_upload_request_v1.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Wasm only. Pass NULL context. No fallback, retry or caller-selected identity.
 * Use with client_call_once / C++ client::call_once; never retain the buffers. */
int32_t mbu_wasm_transport(void *, const uint8_t *, uint32_t, uint8_t *, uint32_t);
/* Publish the exact final validated response once, then return from morrow_run. */
uint32_t mbu_wasm_complete_response(const mbu_response *);
#ifdef __cplusplus
}
#endif
#endif
