#include "morrow_blob_upload_history_v1.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#define REQUIRE(x) do { if (!(x)) return __LINE__; } while (0)
int main(int argc, char **argv) {
  uint8_t wire[MBH_V1_MAX_PACKET_BYTES], submission[32], nonce[32];
  MbhPacket *owner = NULL, *unchanged;
  MbhView view, sentinel;
  FILE *file = NULL;
  size_t length;
  REQUIRE(argc == 2);
  REQUIRE(fopen_s(&file, argv[1], "rb") == 0 && file != NULL);
  length = fread(wire, 1, sizeof wire, file);
  REQUIRE(length > 0 && length < sizeof wire && !ferror(file));
  REQUIRE(fclose(file) == 0);
  memset(submission, 1, sizeof submission);memset(nonce, 9, sizeof nonce);
  REQUIRE(mbh_packet_decode(wire, (uint32_t)length, submission, nonce, &owner) == 0 && owner != NULL);
  memset(&view, 0xa5, sizeof view);memcpy(&sentinel, &view, sizeof view);
  REQUIRE(mbh_packet_view(owner, &view, (uint32_t)sizeof view - 1) == 5);
  REQUIRE(memcmp(&view, &sentinel, sizeof view) == 0);
  REQUIRE(mbh_packet_view(owner, &view, (uint32_t)sizeof view) == 0);
  REQUIRE(view.abi_version == 1 && view.struct_size == sizeof view);
  REQUIRE(view.revision == 3 && view.original_unknown == 1 && view.current_stage == 1);
  REQUIRE(view.outcome == 4 && view.error_code == 7 && view.calls == UINT32_MAX);
  REQUIRE(view.execution == 1 && view.exit_code == INT32_MIN && view.host_calls == UINT32_MAX);
  REQUIRE(view.registry_revision == UINT64_MAX && view.wire_bytes == UINT64_MAX && view.native_work_bytes == UINT64_MAX);
  REQUIRE(view.reserved_at == -10 && view.observed_at == 0 && view.acknowledged_at == 1);
  REQUIRE(view.byte_length == 16u * 1024u * 1024u);
  REQUIRE(view.operation_id.bytes == sizeof("upload-原记录") - 1);
  REQUIRE(memcmp(view.operation_id.data, "upload-原记录", view.operation_id.bytes) == 0);
  REQUIRE(memcmp(view.submission, submission, 32) == 0 && memcmp(view.query_nonce, nonce, 32) == 0);
  {
    uint8_t task_id[MBH_V1_TASK_ID_BYTES];uint32_t written = 0;
    MbhPacket *task_owner = NULL, *prior;
    REQUIRE(mbh_task_id_write(submission, nonce, task_id, sizeof task_id, &written) == 0 && written == sizeof task_id);
    REQUIRE(mbh_packet_decode_task(wire, (uint32_t)length, task_id, written, &task_owner) == 0);
    REQUIRE(mbh_packet_view(task_owner, &view, sizeof view) == 0 && view.original_unknown == 1 && view.current_stage == 1);
    prior = task_owner;task_id[11] = 'f';
    REQUIRE(mbh_packet_decode_task(wire, (uint32_t)length, task_id, written, &task_owner) == 4 && task_owner == prior);
    mbh_packet_free(task_owner);
  }
  unchanged = owner;nonce[0] = 8;
  REQUIRE(mbh_packet_decode(wire, (uint32_t)length, submission, nonce, &owner) == 4 && owner == unchanged);
  REQUIRE(mbh_packet_decode(wire, (uint32_t)length - 1, submission, nonce, &owner) != 0 && owner == unchanged);
  mbh_packet_free(owner);mbh_packet_free(NULL);
  puts("C_HISTORY_CONSUMER_PASS");return 0;
}
