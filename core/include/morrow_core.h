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
#ifdef __cplusplus
}
#endif
#endif
