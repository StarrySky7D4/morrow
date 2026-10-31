#include "morrow_blob_upload_history_task_v1.h"
#include <stdio.h>
#include <string.h>
#define REQUIRE(x) do { if (!(x)) return __LINE__; } while (0)
static int read_task(const char *name, uint8_t *bytes, uint32_t *length) {
  FILE *file = NULL;size_t count;
  if (fopen_s(&file, name, "rb") != 0 || file == NULL) return 0;
  count = fread(bytes, 1, MBH_V1_MAX_TASK_BYTES, file);
  if (!count || count >= MBH_V1_MAX_TASK_BYTES || ferror(file)) { fclose(file);return 0; }
  if (fclose(file) != 0) return 0;
  *length = (uint32_t)count;return 1;
}
int main(int argc, char **argv) {
  static uint8_t bytes[MBH_V1_MAX_TASK_BYTES];uint32_t length;
  const char *handler = "history.inspect", *type = "morrow.blob.upload.history.v1";
  MbhPacket *owner = NULL, *prior;MbhView before = {0}, after = {0};
  REQUIRE(argc == 3 && read_task(argv[1], bytes, &length));
  REQUIRE(mbh_packet_decode_invocation(bytes, length, (const uint8_t*)handler, (uint32_t)strlen(handler),
    (const uint8_t*)type, (uint32_t)strlen(type), &owner) == 0 && owner != NULL);
  REQUIRE(mbh_packet_view(owner, &before, sizeof before) == 0);
  REQUIRE(before.original_unknown == 1 && before.current_stage == 1 && before.revision == 3);
  REQUIRE(before.outcome == 4 && before.error_code == 7 && before.registry_revision == UINT64_MAX);
  REQUIRE(before.operation_id.bytes == strlen("upload-原记录"));
  REQUIRE(memcmp(before.operation_id.data, "upload-原记录", before.operation_id.bytes) == 0);
  prior = owner;REQUIRE(read_task(argv[2], bytes, &length));
  REQUIRE(mbh_packet_decode_invocation(bytes, length, (const uint8_t*)handler, (uint32_t)strlen(handler),
    (const uint8_t*)type, (uint32_t)strlen(type), &owner) == 4 && owner == prior);
  REQUIRE(mbh_packet_view(owner, &after, sizeof after) == 0 && before.operation_id.data == after.operation_id.data);
  mbh_packet_free(owner);puts("C_HISTORY_TASK_CONSUMER_PASS");return 0;
}
