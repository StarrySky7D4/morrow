#include "morrow_plugin_mutation.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static mp_span span(const void *p, size_t n) {
  mp_span result = {(const uint8_t *)p, (uint32_t)n};
  assert(n <= UINT32_MAX);
  return result;
}
static uint32_t load(const char *dir, const char *name, uint8_t *out) {
  char path[1024];
  FILE *file = NULL;
  size_t n;
  int written = snprintf(path, sizeof(path), "%s/%s.bin", dir, name);
  assert(written > 0 && (size_t)written < sizeof(path));
#if defined(_WIN32)
  assert(fopen_s(&file, path, "rb") == 0);
#else
  file = fopen(path, "rb");
#endif
  assert(file != NULL);
  n = fread(out, 1, MP_MAX_MUTATION_FRAME_BYTES + 1u, file);
  assert(!ferror(file) && fgetc(file) == EOF);
  fclose(file);
  assert(n > 0 && n <= MP_MAX_MUTATION_FRAME_BYTES);
  return (uint32_t)n;
}

int main(int argc, char **argv) {
  static const char *valid_requests[] = {
      "create-empty", "create-content", "delete", "chunk", "commit",
      "execute", "query", "cancel", "release"};
  static const char *invalid_requests[] = {
      "bad-zero-reference", "bad-empty-hash", "bad-chunk-overflow",
      "bad-call", "bad-deadline", "bad-unicode-control", "bad-trailing"};
  static const char *valid_responses[] = {"created", "unknown", "denied"};
  static const char *invalid_responses[] = {"bad-success-effect", "bad-denied-effect"};
  static const uint8_t empty_hash[32] = {
      0xe3,0xb0,0xc4,0x42,0x98,0xfc,0x1c,0x14,0x9a,0xfb,0xf4,0xc8,0x99,0x6f,0xb9,0x24,
      0x27,0xae,0x41,0xe4,0x64,0x9b,0x93,0x4c,0xa4,0x95,0x99,0x1b,0x78,0x52,0xb8,0x55};
  static const uint8_t content_hash[32] = {
      0x26,0xa6,0x6b,0x06,0x1e,0x8f,0x48,0xf3,0x99,0x27,0xc3,0x12,0xf2,0x52,0x93,0x95,
      0x97,0x29,0xee,0xe9,0x59,0x78,0xe2,0x89,0x2d,0x49,0xd3,0x51,0x2a,0x5c,0xc0,0x92};
  uint8_t *input, *output, *reply;
  uint8_t reference[32], submission[32], digest[32], chunk_bytes[] = {0,1,255};
  uint32_t n, out_length, execute_length, reply_length;
  mp_mutation_request_v1 req = {0};
  mp_mutation_response *owned = NULL;
  mp_mutation_response_view view = {0};
  size_t i;
  assert(argc == 2);
  input = (uint8_t *)malloc(MP_MAX_MUTATION_FRAME_BYTES + 1u);
  output = (uint8_t *)malloc(MP_MAX_MUTATION_FRAME_BYTES);
  reply = (uint8_t *)malloc(MP_MAX_MUTATION_FRAME_BYTES + 1u);
  assert(input && output && reply);
  assert(mp_mutation_schema_digest(digest, 31) == MP_CODEC_LIMIT);
  assert(mp_mutation_schema_digest(digest, 32) == MP_CODEC_OK);
  for (i = 0; i < sizeof(valid_requests)/sizeof(valid_requests[0]); ++i) {
    n = load(argv[1], valid_requests[i], input);
    assert(mp_mutation_request_validate(input, n) == MP_CODEC_OK);
  }
  for (i = 0; i < sizeof(invalid_requests)/sizeof(invalid_requests[0]); ++i) {
    n = load(argv[1], invalid_requests[i], input);
    assert(mp_mutation_request_validate(input, n) != MP_CODEC_OK);
  }
  assert(mp_mutation_request_validate(input, MP_MAX_MUTATION_FRAME_BYTES + 1u) == MP_CODEC_LIMIT);
  memset(reference, 0x11, sizeof(reference));
  memset(submission, 0x22, sizeof(submission));
  req.abi_version = MP_MUTATION_ABI_VERSION;
  req.struct_size = sizeof(req);
  req.call_id = 7;
  req.reference = span(reference, sizeof(reference));
  req.submission = span(submission, sizeof(submission));
  req.operation_id = span("mutation-vector-1", strlen("mutation-vector-1"));
  req.deadline_ms = MP_MUTATION_MAX_DEADLINE_MS;
  for (req.kind = MP_MUTATION_PREPARE_CREATE;
       req.kind <= MP_MUTATION_RELEASE; ++req.kind) {
    req.content_length = 0; req.content_sha256 = span(NULL, 0);
    req.offset = 0; req.bytes = span(NULL, 0);
    if (req.kind == MP_MUTATION_PREPARE_CREATE)
      req.content_sha256 = span(empty_hash, sizeof(empty_hash));
    if (req.kind == MP_MUTATION_CHUNK)
      req.bytes = span(chunk_bytes, sizeof(chunk_bytes));
    out_length = 99;
    assert(mp_mutation_request_encode(&req, output,
                                     MP_MAX_MUTATION_FRAME_BYTES, &out_length) == MP_CODEC_OK);
    assert(out_length > 0);
    assert(mp_mutation_request_validate(output, out_length) == MP_CODEC_OK);
    if (req.kind == MP_MUTATION_EXECUTE) {
      execute_length = load(argv[1], "execute", input);
      assert(out_length == execute_length && memcmp(output, input, out_length) == 0);
    }
  }
  req.kind = MP_MUTATION_PREPARE_CREATE;
  req.content_length = 3;
  req.content_sha256 = span(content_hash, sizeof(content_hash));
  out_length = 99;
  assert(mp_mutation_request_encode(&req, output, MP_MAX_MUTATION_FRAME_BYTES,
                                    &out_length) == MP_CODEC_OK);
  assert(mp_mutation_request_validate(output, out_length) == MP_CODEC_OK);
  req.content_length = 0; req.content_sha256 = span(empty_hash, sizeof(empty_hash));
  memset(output, 0xa5, MP_MAX_MUTATION_FRAME_BYTES);
  req.reference.length = 31; out_length = 99;
  assert(mp_mutation_request_encode(&req, output, MP_MAX_MUTATION_FRAME_BYTES,
                                    &out_length) == MP_CODEC_INVALID);
  assert(out_length == 0 && output[0] == 0xa5);
  req.reference.length = 32; out_length = 99;
  assert(mp_mutation_request_encode(&req, output, 1, &out_length) == MP_CODEC_LIMIT);
  assert(out_length == 0 && output[0] == 0xa5);
  req.content_length = MP_MAX_MUTATION_CONTENT_BYTES + 1u; out_length = 99;
  assert(mp_mutation_request_encode(&req, output, MP_MAX_MUTATION_FRAME_BYTES,
                                    &out_length) != MP_CODEC_OK && out_length == 0);

  execute_length = load(argv[1], "execute", input);
  for (i = 0; i < sizeof(valid_responses)/sizeof(valid_responses[0]); ++i) {
    reply_length = load(argv[1], valid_responses[i], reply);
    assert(mp_mutation_response_decode(reply, reply_length, input,
                                       execute_length, &owned) == MP_CODEC_OK && owned);
    memset(reply, 0, reply_length);
    assert(mp_mutation_response_get(owned, &view, sizeof(view)) == MP_CODEC_OK);
    assert(view.call_id == 7 && view.kind == MP_MUTATION_EXECUTE);
    assert(view.reference.length == 32 && view.reference.data[0] == 0x11);
    assert(view.submission.length == 32 && view.submission.data[0] == 0x22);
    assert(view.operation_id.length == strlen("mutation-vector-1"));
    assert(view.encoded_frame.length == reply_length && view.encoded_frame.data);
    if (i == 0) assert(view.status == MP_MUTATION_STATUS_COMPLETED &&
                       view.phase == MP_MUTATION_PHASE_OBSERVED &&
                       view.effect == MP_MUTATION_EFFECT_OS_SUCCEEDED);
    if (i == 1) assert(view.status == MP_MUTATION_STATUS_OUTCOME_UNKNOWN &&
                       view.phase == MP_MUTATION_PHASE_OUTCOME_UNKNOWN &&
                       view.effect == MP_MUTATION_EFFECT_UNSPECIFIED);
    if (i == 2) assert(view.status == MP_MUTATION_STATUS_DENIED &&
                       view.phase == MP_MUTATION_PHASE_NONE);
    mp_mutation_response_free(owned); owned = NULL;
  }
  for (i = 0; i < sizeof(invalid_responses)/sizeof(invalid_responses[0]); ++i) {
    reply_length = load(argv[1], invalid_responses[i], reply);
    assert(mp_mutation_response_decode(reply, reply_length, input,
                                       execute_length, &owned) != MP_CODEC_OK && !owned);
  }
  reply_length = load(argv[1], "created", reply);
  req.kind = MP_MUTATION_EXECUTE; req.content_length = 0;
  req.content_sha256 = span(NULL, 0); req.call_id = 8;
  out_length = 0;
  assert(mp_mutation_request_encode(&req, output, MP_MAX_MUTATION_FRAME_BYTES,
                                    &out_length) == MP_CODEC_OK);
  assert(mp_mutation_response_decode(reply, reply_length, output,
                                     out_length, &owned) == MP_CODEC_CORRELATION && !owned);
  assert(mp_mutation_response_get(owned, &view, sizeof(view)) == MP_CODEC_INVALID);
  assert(view.encoded_frame.data == NULL && view.encoded_frame.length == 0);
  free(input); free(output); free(reply);
  puts("PASS_SCOPED C mutation codec 21 vectors, eight actions, bounded owned replies");
  return 0;
}
