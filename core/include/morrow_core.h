#ifndef MORROW_CORE_H
#define MORROW_CORE_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Trusted, same-process client only. Maximum 16 buffers, 65536 bytes each.
 * Handles are never reused in one library/Worker lifetime. Allocation failure: 0.
 * Own handles exclusively. Pointer valid only until free. Do not write during
 * process/free and do not pass these handles across process/Worker restarts.
 * process copies and validates protocol bytes and returns a NEW owned buffer;
 * caller must free input and output. It does NOT authorize or commit an edit.
 * Status: 0 success, 1 contract mismatch, 2 invalid input, 3 limit, 255 stale.
 */
uint32_t morrow_buffer_new(uint32_t length);
uint8_t *morrow_buffer_ptr(uint32_t handle);
uint32_t morrow_buffer_len(uint32_t handle);
uint32_t morrow_buffer_process(uint32_t handle);
uint32_t morrow_buffer_status(uint32_t handle);
uint32_t morrow_buffer_free(uint32_t handle);
uint32_t morrow_buffer_live(void);
/* Native trusted host control plane, unavailable in wasm32 builds.
 * Open only an existing absolute UTF-8 database path from a buffer (no trailing NUL).
 * At most 8 hosts, 128 connections per host. Handles are never reused.
 * A host owns its connections. Closing it invalidates every connection.
 * Only the trusted embedding host may mint/select these handles and grant capabilities.
 * Capabilities: 1 rename, 2 summary read, 3 scoped operation-result query.
 * TTL is relative milliseconds; Rust owns the monotonic clock. Zero TTL is rejected.
 * Open/grant/revoke/dispatch failures return 0 and set the input buffer status.
 * Added statuses: 4 native/backend failure, 5 busy; 255 invalid host/connection.
 * Successful dispatch returns a NEW owned binary-response buffer; free both buffers.
 * Dispatch reserves its response slot before execution. A lost response still does
 * not prove rollback: query the stable operation ID with appropriate authorization.
 * No create/import/migration entry is exported. Native FFI is not a plugin sandbox.
 */
uint32_t morrow_host_open(uint32_t path_buffer);
uint32_t morrow_host_close(uint32_t host);
uint32_t morrow_host_connect(uint32_t host);
uint32_t morrow_host_disconnect(uint32_t host, uint32_t connection);
uint32_t morrow_host_grant(uint32_t host, uint32_t connection, uint32_t capability, uint32_t card_buffer, uint32_t ttl_ms);
uint32_t morrow_host_revoke(uint32_t host, uint32_t connection, uint32_t capability, uint32_t card_buffer);
uint32_t morrow_host_dispatch(uint32_t host, uint32_t connection, uint32_t input);
uint32_t morrow_host_live(void);
#ifdef __cplusplus
}
#endif
#endif
