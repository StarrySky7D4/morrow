#ifndef MORROW_BLOB_UPLOAD_HISTORY_V1_H
#define MORROW_BLOB_UPLOAD_HISTORY_V1_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MBH_V1_MAX_PACKET_BYTES 8192u
#define MBH_V1_TASK_ID_BYTES 140u
typedef struct MbhPacket MbhPacket;
typedef struct MbhTextView { const uint8_t *data; uint32_t bytes, reserved; } MbhTextView;
typedef struct MbhView {
  uint32_t abi_version, struct_size;
  uint8_t query_nonce[32], submission[32], package_digest[32], whole_sha256[32];
  uint8_t input_sha256[32], request_binding_sha256[32];
  MbhTextView package_id, operation_id, handler;
  uint64_t registry_revision, byte_length;
  int64_t reserved_at, observed_at, acknowledged_at;
  uint64_t wire_bytes, native_work_bytes;
  uint32_t revision, current_stage, original_unknown, outcome, error_code, calls;
  uint32_t execution;
  int32_t exit_code;
  uint32_t execution_error, host_calls;
} MbhView;
/* Status: 0 success, 1 invalid, 2 contract, 3 limit, 4 correlation, 5 buffer.
 * The caller supplies valid readable inputs and an aligned writable output
 * disjoint from all inputs. Failure preserves the output slot. No retry occurs.
 * Arrays submission/nonce each contain exactly 32 bytes. */
uint32_t mbh_packet_decode(const uint8_t *, uint32_t, const uint8_t *, const uint8_t *, MbhPacket **);
/* Canonical existing-task correlation, not approval. No trailing NUL is written.
 * Output and written slots must be aligned where applicable, mutually disjoint,
 * and disjoint from the readable 32-byte submission and nonce inputs.
 * Reported failures preserve both output slots. */
uint32_t mbh_task_id_write(const uint8_t *, const uint8_t *, uint8_t *, uint32_t, uint32_t *);
/* task_id is the separately validated invocation ID, not taken from the packet.
 * Only exact lowercase canonical IDs are accepted. Failure preserves the owner.
 * The same memory and ownership obligations as mbh_packet_decode apply. */
uint32_t mbh_packet_decode_task(const uint8_t *, uint32_t, const uint8_t *, uint32_t, MbhPacket **);
/* Output must not overlap owner/borrowed storage; strings live until owner free.
 * revision: 1 reserved, 2 observed, 3 acknowledged. current_stage: 0/1/2.
 * outcome: 0 unobserved, 1 absent, 2 staged, 3 released, 4 original error.
 * execution: 0 unobserved, 1 returned (signed exit_code), 2 rejected (1..9).
 * original_unknown stays independent of acknowledgement and current_stage. */
uint32_t mbh_packet_view(const MbhPacket *, MbhView *, uint32_t);
void mbh_packet_free(MbhPacket *);
#ifdef __cplusplus
}
#endif
#endif
