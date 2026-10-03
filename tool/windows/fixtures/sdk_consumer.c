#ifdef NDEBUG
#error Consumer assertions must be enabled
#endif
#include "morrow_channel_v1.h"
#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
_Static_assert(sizeof(void *) == 8, "This qualification is Windows x64 only");
_Static_assert(sizeof(mp_channel_host_v1) == 24, "Rust HostV1 ABI size");
_Static_assert(offsetof(mp_channel_host_v1, context) == 8, "Rust HostV1 context offset");
_Static_assert(offsetof(mp_channel_host_v1, call) == 16, "Rust HostV1 callback offset");
typedef struct state {
  uint8_t *request, *reply;
  uint32_t request_length, reply_length, calls, failure;
} state;
static uint8_t *load(const char *directory, const char *name, uint32_t *length) {
  char path[4096];
  int n = snprintf(path, sizeof(path), "%s/%s", directory, name);
  assert(n > 0 && (size_t)n < sizeof(path));
  FILE *file = NULL;
  assert(fopen_s(&file, path, "rb") == 0 && file);
  assert(fseek(file, 0, SEEK_END) == 0);
  long size = ftell(file);
  assert(size > 0 && (size_t)size <= MP_MAX_CHANNEL_WIRE_BYTES);
  size_t count = (size_t)size;
  rewind(file);
  uint8_t *bytes = (uint8_t *)malloc(count);
  assert(bytes && fread(bytes, 1, count, file) == count);
  assert(fclose(file) == 0);
  *length = (uint32_t)count;
  return bytes;
}
static uint32_t callback(void *context, const uint8_t *input, uint32_t length,
                         uint8_t *output, uint32_t capacity, uint32_t *written) {
  state *s = (state *)context;
  assert(mp_channel_request_validate(input, length) == MP_CODEC_OK);
  assert(length == s->request_length && memcmp(input, s->request, length) == 0);
  assert(capacity == MP_MAX_CHANNEL_WIRE_BYTES && s->reply_length <= capacity);
  assert((uintptr_t)input + length <= (uintptr_t)output ||
         (uintptr_t)output + capacity <= (uintptr_t)input);
  ++s->calls;
  memset(output, 0xa5, capacity);
  memcpy(output, s->reply, s->reply_length);
  *written = s->reply_length;
  return s->failure;
}
static void verify_owned(const mp_channel_response *response, const uint8_t *digest) {
  mp_channel_response_view view = {0};
  assert(mp_channel_response_get(response, &view, sizeof(view)) == MP_CODEC_OK);
  assert(view.status == MP_CHANNEL_FRAME && view.has_frame == 1);
  assert(view.sequence == 1 && view.bytes.length == 65536 && view.cursor.length == 4);
  assert(view.call_id.length == 32 && view.reference.length == 32 && view.source_epoch.length == 32);
  assert(view.frame_sha256.length == 32 && memcmp(view.frame_sha256.data, digest, 32) == 0);
  for (uint32_t i = 0; i < 32; ++i) {
    assert(view.call_id.data[i] == 1 && view.reference.data[i] == 2 && view.source_epoch.data[i] == 3);
  }
  for (uint32_t i = 0; i < view.bytes.length; ++i) assert(view.bytes.data[i] == (uint8_t)(i % 251));
  const uint8_t cursor[4] = {0, 255, 1, 2};
  assert(memcmp(view.cursor.data, cursor, sizeof(cursor)) == 0);
}
int main(int argc, char **argv) {
  assert(argc == 2);
  state s = {0};
  s.request = load(argv[1], "receive.request.capnp", &s.request_length);
  s.reply = load(argv[1], "receive.response.capnp", &s.reply_length);
  uint32_t digest_length = 0;
  uint8_t *digest = load(argv[1], "frame.sha256", &digest_length);
  assert(digest_length == 32);
  uint8_t call[32], reference[32], epoch[32];
  memset(call, 1, sizeof(call)); memset(reference, 2, sizeof(reference)); memset(epoch, 3, sizeof(epoch));
  mp_channel_request_v1 request = {0};
  request.abi_version = 1; request.struct_size = sizeof(request); request.kind = MP_CHANNEL_RECEIVE;
  request.call_id.data = call; request.call_id.length = 32;
  request.reference.data = reference; request.reference.length = 32;
  request.source_epoch.data = epoch; request.source_epoch.length = 32;
  request.credit_bytes = 65536;
  mp_channel_host_v1 host = {1, sizeof(host), &s, callback};
  mp_channel_response *first = NULL, *second = NULL, *failed = NULL;
  assert(mp_channel_call(&host, &request, &first) == MP_CODEC_OK && first && s.calls == 1);
  assert(mp_channel_call(&host, &request, &second) == MP_CODEC_OK && second && s.calls == 2);
  verify_owned(first, digest); verify_owned(second, digest);
  /* Source bytes may change after callback return; response-owned views remain valid. */
  memset(s.reply, 0, s.reply_length);
  verify_owned(first, digest); verify_owned(second, digest);
  s.failure = 9;
  assert(mp_channel_call(&host, &request, &failed) == MP_TRANSPORT_FAILURE);
  assert(failed == NULL && s.calls == 3); /* one submission, no implicit retry */
  host.abi_version = 2;
  assert(mp_channel_call(&host, &request, &failed) == MP_ABI_MISMATCH);
  assert(failed == NULL && s.calls == 3); /* bad ABI never invokes callback */
  free(s.reply); free(s.request);
  verify_owned(first, digest); verify_owned(second, digest);
  mp_channel_response_free(first); mp_channel_response_free(second); free(digest);
  printf("COUNTS c successes=2 failures=1 calls=%u rejected=1\n", s.calls);
  puts("PASS standalone C11: owned64KiB, failure/no retry, ABI rejection");
  return 0;
}
