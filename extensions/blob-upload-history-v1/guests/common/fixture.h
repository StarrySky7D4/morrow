#ifndef MORROW_HISTORY_FIXTURE_H
#define MORROW_HISTORY_FIXTURE_H
#include "morrow_blob_upload_history_v1.h"
#include "morrow_plugin_task.h"
#include <limits.h>
#include <string.h>
static inline int fixture_text(const uint8_t *p, uint32_t n, const char *s) {
  return n == strlen(s) && memcmp(p, s, n) == 0;
}
static inline int fixture_array(const uint8_t *p, uint8_t b) {
  uint32_t i;for (i = 0; i != 32; ++i) if (p[i] != b) return 0;return 1;
}
static inline int fixture_transform(const mp_transform_view *v) {
  return fixture_text(v->handler.data, v->handler.length, "history.fixture.inspect") &&
    fixture_text(v->input_type.data, v->input_type.length, "morrow.blob.upload.history.v1") &&
    fixture_text(v->output_type.data, v->output_type.length, "morrow.blob.upload.history.v1");
}
static inline int fixture_history(const MbhView *v) {
  if (v->abi_version != 1 || v->struct_size != sizeof(*v) ||
      !fixture_array(v->submission, 1) || !fixture_array(v->query_nonce, 9) ||
      !fixture_array(v->package_digest, 2) || !fixture_array(v->whole_sha256, 3) ||
      !fixture_array(v->input_sha256, 4) || !fixture_array(v->request_binding_sha256, 5) ||
      !fixture_text(v->package_id.data, v->package_id.bytes, "org.example.history") ||
      !fixture_text(v->operation_id.data, v->operation_id.bytes, "upload-原记录") ||
      !fixture_text(v->handler.data, v->handler.bytes, "inspect-history") ||
      v->registry_revision != UINT64_MAX || v->byte_length != 16u * 1024u * 1024u ||
      v->reserved_at != -10 || v->current_stage != 1 || v->original_unknown != 1) return 0;
  if (v->revision == 1) return v->outcome == 0 && v->execution == 0 &&
    v->observed_at == 0 && v->acknowledged_at == 0 && v->calls == 0 &&
    v->wire_bytes == 0 && v->native_work_bytes == 0;
  return v->revision == 3 && v->outcome == 4 && v->error_code == 7 &&
    v->calls == UINT32_MAX && v->wire_bytes == UINT64_MAX && v->native_work_bytes == UINT64_MAX &&
    v->execution == 1 && v->exit_code == INT32_MIN && v->host_calls == UINT32_MAX &&
    v->observed_at == 0 && v->acknowledged_at == 1;
}
#endif
