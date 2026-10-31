#ifndef MORROW_BLOB_UPLOAD_HISTORY_TASK_V1_H
#define MORROW_BLOB_UPLOAD_HISTORY_TASK_V1_H
#include "morrow_blob_upload_history_v1.h"
#define MBH_V1_MAX_TASK_BYTES 131072u
#ifdef __cplusplus
extern "C" {
#endif
/* Link the additive task-adapter library. Pass the original Task envelope and
 * the receiving plugin's declared handler/output type (UTF-8, at most 256 bytes).
 * The Task codec validates the actual envelope ID, kind, version, digest and
 * input type; the history codec then correlates that ID with the selected packet.
 * No expected ID is reconstructed from packet data. Nothing here grants access.
 * Inputs remain readable for the call. The aligned output slot is exclusively
 * writable and disjoint from all inputs. Failure preserves its previous value.
 * Successful owners use the original mbh_packet_view/mbh_packet_free functions. */
uint32_t mbh_packet_decode_invocation(const uint8_t *, uint32_t,
    const uint8_t *, uint32_t, const uint8_t *, uint32_t, MbhPacket **);
#ifdef __cplusplus
}
#endif
#endif
