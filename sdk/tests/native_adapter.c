/* Trusted Windows qualification host. This file is NOT included in the guest
 * SDK. */
#include "morrow_plugin_sdk.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>
typedef uint32_t (*one_fn)(uint32_t);
typedef uint32_t (*two_fn)(uint32_t, uint32_t);
typedef uint32_t (*three_fn)(uint32_t, uint32_t, uint32_t);
typedef uint32_t (*grant_fn)(uint32_t, uint32_t, uint32_t, uint32_t, uint32_t);
typedef uint8_t *(*ptr_fn)(uint32_t);
typedef uint32_t (*live_fn)(void);
struct adapter {
  one_fn alloc, free_buffer, len;
  ptr_fn ptr;
  three_fn dispatch;
  uint32_t host, connection, calls;
};
static uint32_t dispatch(void *context, const uint8_t *input, uint32_t length,
                         uint8_t *output, uint32_t capacity,
                         uint32_t *written) {
  struct adapter *a = (struct adapter *)context;
  uint32_t in = a->alloc(length), out, size;
  ++a->calls;
  if (!in)
    return 1;
  memcpy(a->ptr(in), input, length);
  out = a->dispatch(a->host, a->connection, in);
  a->free_buffer(in);
  if (!out)
    return 1;
  size = a->len(out);
  if (size > capacity) {
    a->free_buffer(out);
    return 1;
  }
  memcpy(output, a->ptr(out), size);
  *written = size;
  a->free_buffer(out);
  return 0;
}
static void write_reply(const char *directory, const char *name,
                        const uint8_t *bytes, uint32_t length) {
  char path[4096];
  FILE *f;
  int n = snprintf(path, sizeof(path), "%s/%s", directory, name);
  assert(n > 0 && (size_t)n < sizeof(path));
  assert(fopen_s(&f, path, "wb") == 0);
  assert(f);
  assert(fwrite(bytes, 1, length, f) == length);
  assert(fclose(f) == 0);
}
int main(int argc, char **argv) {
  HMODULE dll;
  struct adapter a;
  one_fn open_host, close_host, connect;
  two_fn disconnect;
  grant_fn grant;
  live_fn live;
  mp_host_v1 guest;
  uint8_t *request, *output, *first;
  uint32_t handle, length, written, first_length;
  long bytes;
  FILE *f;
  assert(argc == 5);
  dll = LoadLibraryA(argv[1]);
  assert(dll);
  memset(&a, 0, sizeof(a));
#define LOAD(type, name) ((type)(void *)GetProcAddress(dll, name))
  a.alloc = LOAD(one_fn, "morrow_buffer_new");
  a.free_buffer = LOAD(one_fn, "morrow_buffer_free");
  a.len = LOAD(one_fn, "morrow_buffer_len");
  a.ptr = LOAD(ptr_fn, "morrow_buffer_ptr");
  a.dispatch = LOAD(three_fn, "morrow_host_dispatch");
  open_host = LOAD(one_fn, "morrow_host_open");
  close_host = LOAD(one_fn, "morrow_host_close");
  connect = LOAD(one_fn, "morrow_host_connect");
  disconnect = LOAD(two_fn, "morrow_host_disconnect");
  grant = LOAD(grant_fn, "morrow_host_grant");
  live = LOAD(live_fn, "morrow_buffer_live");
  assert(a.alloc && a.free_buffer && a.len && a.ptr && a.dispatch &&
         open_host && close_host && connect && disconnect && grant && live);
  length = (uint32_t)strlen(argv[2]);
  handle = a.alloc(length);
  assert(handle);
  memcpy(a.ptr(handle), argv[2], length);
  a.host = open_host(handle);
  a.free_buffer(handle);
  assert(a.host);
  a.connection = connect(a.host);
  assert(a.connection);
  assert(fopen_s(&f, argv[3], "rb") == 0);
  assert(f);
  assert(fseek(f, 0, SEEK_END) == 0);
  bytes = ftell(f);
  assert(bytes > 0 && bytes <= (long)MP_MAX_MESSAGE_BYTES);
  rewind(f);
  length = (uint32_t)bytes;
  request = (uint8_t *)malloc(length);
  assert(request);
  assert(fread(request, 1, length, f) == length);
  fclose(f);
  output = (uint8_t *)malloc(MP_MAX_MESSAGE_BYTES);
  first = (uint8_t *)malloc(MP_MAX_MESSAGE_BYTES);
  assert(output && first);
  guest.abi_version = 1;
  guest.struct_size = sizeof(guest);
  guest.context = &a;
  guest.exchange = dispatch;
  assert(mp_exchange(&guest, request, length, output, MP_MAX_MESSAGE_BYTES,
                     &written) == MP_OK);
  write_reply(argv[4], "denied.capnp", output, written);
  handle = a.alloc(10);
  assert(handle);
  memcpy(a.ptr(handle), "legacy-123", 10);
  assert(grant(a.host, a.connection, 1, handle, 60000));
  a.free_buffer(handle);
  assert(mp_exchange(&guest, request, length, output, MP_MAX_MESSAGE_BYTES,
                     &written) == MP_OK);
  write_reply(argv[4], "committed.capnp", output, written);
  first_length = written;
  memcpy(first, output, written);
  assert(mp_exchange(&guest, request, length, output, MP_MAX_MESSAGE_BYTES,
                     &written) == MP_OK);
  assert(written == first_length && memcmp(first, output, written) == 0);
  assert(a.calls == 3);
  assert(disconnect(a.host, a.connection));
  assert(close_host(a.host));
  assert(live() == 0);
  free(first);
  free(output);
  free(request);
  FreeLibrary(dll);
  puts("PASS: C SDK -> trusted adapter -> actual Rust DLL/SQLite, denied "
       "response and duplicate commit, zero buffer leaks");
  return 0;
}
