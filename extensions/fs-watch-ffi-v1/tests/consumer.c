#define _CRT_SECURE_NO_WARNINGS
#include "morrow_fs_watch_v1.h"
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

_Static_assert(sizeof(mfw_frame_info_v1) == 64, "frame ABI");
_Static_assert(offsetof(mfw_frame_info_v1, sequence) == 40, "sequence ABI");
_Static_assert(sizeof(mfw_state_limits_v1) == 32, "limits ABI");
_Static_assert(sizeof(mfw_state_snapshot_v1) == 40, "snapshot ABI");
_Static_assert(offsetof(mfw_change_view_v1, name) == 8, "borrowed name ABI");
_Static_assert(sizeof(mfw_change_view_v1) == (sizeof(void *) == 8 ? 24 : 20),
               "change ABI");

static unsigned checks = 0, scenarios = 0;
#define CHECK(value)                                                           \
  do {                                                                         \
    ++checks;                                                                  \
    if (!(value)) {                                                            \
      fprintf(stderr, "C_FAILURE line=%u expression=%s\n", (unsigned)__LINE__, \
              #value);                                                         \
      exit(2);                                                                 \
    }                                                                          \
  } while (0)
#define OK(value) CHECK((value) == MFW_OK)
typedef struct wire {
  uint8_t *data;
  uint32_t size;
} wire;
static void passed(const char *name) {
  ++scenarios;
  printf("C_SCENARIO %s PASS\n", name);
}
static int all_bytes(const uint8_t *data, size_t length, uint8_t value) {
  size_t n;
  for (n = 0; n < length; n++)
    if (data[n] != value)
      return 0;
  return 1;
}
static wire read_wire(const char *path) {
  FILE *file = fopen(path, "rb");
  long size;
  wire value;
  CHECK(file != NULL);
  CHECK(fseek(file, 0, SEEK_END) == 0);
  size = ftell(file);
  CHECK(size >= 16 && size <= (long)MFW_MAX_WIRE_BYTES);
  CHECK(fseek(file, 0, SEEK_SET) == 0);
  value.size = (uint32_t)size;
  value.data = (uint8_t *)malloc(value.size);
  CHECK(value.data != NULL);
  CHECK(fread(value.data, 1, value.size, file) == value.size);
  CHECK(fgetc(file) == EOF);
  CHECK(fclose(file) == 0);
  return value;
}
static wire synthetic(const char *directory, const char *name) {
  char path[2048];
  int n = snprintf(path, sizeof(path), "%s/%s", directory, name);
  CHECK(n > 0 && (size_t)n < sizeof(path));
  return read_wire(path);
}
static mfw_frame_v1 *decode(wire value) {
  mfw_frame_v1 *result = NULL;
  OK(mfw_frame_v1_decode(value.data, value.size, &result));
  CHECK(result != NULL);
  return result;
}
static mfw_frame_info_v1 info(mfw_frame_v1 *frame) {
  mfw_frame_info_v1 value;
  OK(mfw_frame_v1_info(frame, &value, sizeof(value)));
  CHECK(value.abi_version == 1 && value.struct_size == sizeof(value) &&
        value.reserved == 0);
  return value;
}
static mfw_state_snapshot_v1 snapshot(mfw_state_v1 *owner) {
  mfw_state_snapshot_v1 v;
  OK(mfw_state_v1_snapshot(owner, &v, sizeof(v)));
  CHECK(v.abi_version == 1 && v.struct_size == sizeof(v) && v.reserved == 0);
  return v;
}
static mfw_state_v1 *state(uint8_t epoch_value, uint32_t frames,
                           uint32_t changes, uint64_t bytes) {
  uint8_t epoch[32];
  mfw_state_v1 *out = NULL;
  mfw_state_limits_v1 limits = {1, sizeof(limits), frames, changes, bytes, 0};
  memset(epoch, epoch_value, sizeof(epoch));
  OK(mfw_state_v1_new(epoch, &limits, sizeof(limits), &out));
  CHECK(out != NULL);
  return out;
}
static void ack(mfw_state_v1 *owner, wire w, uint64_t sequence) {
  uint8_t digest[32];
  OK(mfw_wire_v1_digest(w.data, w.size, digest, sizeof(digest)));
  OK(mfw_state_v1_ack(owner, sequence, digest));
}
static mfw_frame_v1 *receive(mfw_state_v1 *owner, wire w) {
  mfw_frame_v1 *f = NULL;
  OK(mfw_state_v1_receive(owner, w.data, w.size, &f));
  CHECK(f != NULL);
  return f;
}
static void roundtrip(mfw_frame_v1 *frame, wire original) {
  uint8_t output[MFW_MAX_WIRE_BYTES];
  uint32_t written = 0;
  OK(mfw_frame_v1_encode(frame, output, sizeof(output), &written));
  CHECK(written == original.size);
  CHECK(memcmp(output, original.data, written) == 0);
}

int main(int argc, char **argv) {
  wire native[6], terminal, gap, unpaired;
  unsigned i, j;
  uint32_t mask = 0, total_changes = 0;
  uint64_t total_bytes = 0;
  const uint8_t old_name[] = {0xea, 0x96, 0x20, 0, 0x3d, 0xd8, 0,    0xde,
                              0x2e, 0,    0x74, 0, 0x78, 0,    0x74, 0};
  const uint8_t new_name[] = {0x72, 0, 0x65, 0, 0x6e, 0, 0x61, 0,    0x6d, 0,
                              0x65, 0, 0x64, 0, 0x20, 0, 0x3d, 0xd8, 0,    0xde,
                              0x2e, 0, 0x74, 0, 0x78, 0, 0x74, 0};
  const uint8_t pending_name[] = {0x70, 0, 0x65, 0, 0x6e, 0, 0x64, 0,
                                  0x69, 0, 0x6e, 0, 0x67, 0, 0x2e, 0,
                                  0x74, 0, 0x78, 0, 0x74, 0};
  const uint8_t schema_expected[32] = {
      0x33, 0x12, 0xe3, 0x34, 0xf5, 0xd2, 0x98, 0xd3, 0xc5, 0x6d, 0xf9,
      0xc0, 0x6e, 0xd3, 0xac, 0x94, 0x0b, 0xbf, 0x8f, 0xc9, 0xa0, 0x58,
      0xd1, 0x13, 0xac, 0x30, 0xb0, 0xd6, 0x85, 0x4f, 0x34, 0x6f};
  CHECK(argc == 8);
  for (i = 0; i < 6; i++)
    native[i] = read_wire(argv[i + 1]);
  terminal = synthetic(argv[7], "after-events.capnp");
  gap = synthetic(argv[7], "terminal-2.capnp");
  unpaired = synthetic(argv[7], "unpaired.capnp");
  {
    uint8_t digest[32];
    OK(mfw_schema_v1_digest(digest, sizeof(digest)));
    CHECK(memcmp(digest, schema_expected, 32) == 0);
  }
  for (i = 0; i < 6; i++) {
    mfw_frame_v1 *f = decode(native[i]);
    mfw_frame_info_v1 v = info(f);
    CHECK(v.sequence == (i < 5 ? i + 1 : 1) && v.end == MFW_CONTINUE &&
          v.changes == (i == 3 ? 2u : 1u));
    for (j = 0; j < 32; j++)
      CHECK(v.epoch[j] == 0x42);
    for (j = 0; j < v.changes; j++) {
      mfw_change_view_v1 c;
      const uint8_t *expected;
      size_t length;
      OK(mfw_frame_v1_change(f, j, &c, sizeof(c)));
      CHECK(c.kind >= 1 && c.kind <= 5 && c.reserved == 0 && c.reserved2 == 0);
      if (i == 5) {
        expected = pending_name;
        length = sizeof(pending_name);
      } else if (c.kind == MFW_RENAME_NEW_NAME || c.kind == MFW_REMOVED) {
        expected = new_name;
        length = sizeof(new_name);
      } else {
        expected = old_name;
        length = sizeof(old_name);
      }
      CHECK(c.name_bytes == length && memcmp(c.name, expected, length) == 0);
      if (i < 5)
        mask |= 1u << c.kind;
    }
    roundtrip(f, native[i]);
    mfw_frame_v1_free(f);
  }
  CHECK(mask == 62);
  passed("six-original-native-payloads-raw-names-roundtrip");
  {
    mfw_state_v1 *s = state(0x42, 10, 100, 65536);
    uint8_t digest[32], wrong[32];
    for (i = 0; i < 5; i++) {
      mfw_frame_v1 *f = receive(s, native[i]);
      mfw_frame_info_v1 v = info(f);
      mfw_frame_v1 *unchanged = f;
      mfw_state_snapshot_v1 before;
      total_bytes += native[i].size;
      total_changes += v.changes;
      before = snapshot(s);
      CHECK(before.phase == MFW_PENDING_PHASE && before.frames == i + 1 &&
            before.changes == total_changes &&
            before.wire_bytes == total_bytes);
      CHECK(mfw_state_v1_receive(s, native[i].data, native[i].size,
                                 &unchanged) == MFW_PENDING &&
            unchanged == f);
      OK(mfw_wire_v1_digest(native[i].data, native[i].size, digest, 32));
      memcpy(wrong, digest, 32);
      wrong[0] ^= 1;
      CHECK(mfw_state_v1_ack(s, v.sequence + 1, digest) == MFW_CORRELATION);
      CHECK(mfw_state_v1_ack(s, v.sequence, wrong) == MFW_CORRELATION);
      CHECK(snapshot(s).phase == MFW_PENDING_PHASE);
      OK(mfw_state_v1_ack(s, v.sequence, digest));
      CHECK(snapshot(s).phase == MFW_READY_PHASE);
      CHECK(mfw_state_v1_ack(s, v.sequence, digest) == MFW_STATE);
      mfw_frame_v1_free(f);
    }
    {
      mfw_frame_v1 *f = receive(s, terminal);
      mfw_frame_info_v1 v = info(f);
      CHECK(v.end == MFW_WINDOW_ENDED && v.sequence == 6 && v.changes == 0);
      ack(s, terminal, 6);
      mfw_frame_v1_free(f);
    }
    {
      mfw_state_snapshot_v1 v = snapshot(s);
      CHECK(v.phase == MFW_ENDED_PHASE && v.end == MFW_WINDOW_ENDED &&
            v.frames == 6 && v.wire_bytes == total_bytes + terminal.size);
    }
    OK(mfw_state_v1_transport_lost(s));
    CHECK(snapshot(s).phase == MFW_ENDED_PHASE);
    {
      mfw_frame_v1 *unchanged = NULL;
      CHECK(mfw_state_v1_receive(s, native[0].data, native[0].size,
                                 &unchanged) == MFW_STATE &&
            unchanged == NULL);
    }
    mfw_state_v1_free(s);
    passed("original-wire-ack-correlation-and-terminal-history");
  }
  {
    mfw_state_v1 *s = state(0x42, 10, 100, 65536);
    mfw_frame_v1 *f = receive(s, native[5]);
    uint8_t digest[32];
    OK(mfw_state_v1_transport_lost(s));
    CHECK(snapshot(s).phase == MFW_UNKNOWN_PHASE && snapshot(s).frames == 1 &&
          snapshot(s).wire_bytes == native[5].size);
    OK(mfw_wire_v1_digest(native[5].data, native[5].size, digest, 32));
    CHECK(mfw_state_v1_ack(s, 1, digest) == MFW_UNKNOWN);
    mfw_frame_v1_free(f);
    mfw_state_v1_free(s);
    passed("lost-ack-retains-original-charge");
  }
  for (i = 0; i < 3; i++) {
    mfw_state_v1 *s = state(0x42, i == 0 ? 1u : 10u, i == 1 ? 1u : 100u,
                            i == 2 ? native[0].size : 65536u);
    mfw_frame_v1 *f = receive(s, native[0]);
    mfw_frame_v1 *unchanged = f;
    ack(s, native[0], 1);
    CHECK(mfw_state_v1_receive(s, native[1].data, native[1].size, &unchanged) ==
              MFW_BUDGET &&
          unchanged == f);
    CHECK(snapshot(s).phase == MFW_UNKNOWN_PHASE && snapshot(s).frames == 1 &&
          snapshot(s).wire_bytes == native[0].size);
    mfw_frame_v1_free(f);
    mfw_state_v1_free(s);
  }
  passed("frame-change-byte-budgets-no-refund");
  {
    mfw_frame_v1 *keep = decode(native[0]);
    uint8_t *oversized = (uint8_t *)calloc(MFW_MAX_WIRE_BYTES + 1u, 1);
    CHECK(oversized != NULL);
    for (i = 0; i < 3; i++) {
      mfw_state_v1 *s = state(0x42, 10, 100, 65536);
      mfw_frame_v1 *unchanged = keep;
      uint8_t *bytes = native[0].data;
      uint32_t count = native[0].size - 1;
      uint32_t code;
      if (i == 1) {
        bytes = oversized;
        count = MFW_MAX_WIRE_BYTES + 1u;
      }
      if (i == 2) {
        bytes = oversized;
        count = native[0].size;
      }
      code = mfw_state_v1_receive(s, bytes, count, &unchanged);
      CHECK(code != MFW_OK && unchanged == keep);
      CHECK(snapshot(s).phase == MFW_UNKNOWN_PHASE && snapshot(s).frames == 0);
      CHECK(mfw_state_v1_receive(s, native[0].data, native[0].size,
                                 &unchanged) == MFW_UNKNOWN);
      mfw_state_v1_free(s);
    }
    {
      mfw_state_v1 *s = state(0x42, 10, 100, 65536);
      mfw_frame_v1 *f = receive(s, native[0]);
      mfw_frame_v1 *unchanged = keep;
      CHECK(mfw_state_v1_receive(s, oversized, MFW_MAX_WIRE_BYTES + 1u,
                                 &unchanged) == MFW_PENDING &&
            unchanged == keep);
      CHECK(snapshot(s).phase == MFW_PENDING_PHASE);
      mfw_frame_v1_free(f);
      mfw_state_v1_free(s);
    }
    {
      mfw_frame_v1 *unchanged = keep;
      CHECK(mfw_frame_v1_decode(native[0].data, native[0].size - 1,
                                &unchanged) != MFW_OK &&
            unchanged == keep);
      CHECK(mfw_frame_v1_decode(oversized, MFW_MAX_WIRE_BYTES + 1u,
                                &unchanged) == MFW_LIMIT &&
            unchanged == keep);
    }
    free(oversized);
    mfw_frame_v1_free(keep);
    passed("malformed-oversized-and-phase-precedence");
  }
  {
    wire changed;
    unsigned matches = 0;
    changed.size = native[0].size;
    changed.data = (uint8_t *)malloc(changed.size);
    CHECK(changed.data != NULL);
    memcpy(changed.data, native[0].data, changed.size);
    for (i = 0; i + 32 <= changed.size; i++)
      if (memcmp(changed.data + i, schema_expected, 32) == 0) {
        changed.data[i] ^= 1;
        matches++;
      }
    CHECK(matches == 1);
    {
      mfw_frame_v1 *unchanged = NULL;
      CHECK(mfw_frame_v1_decode(changed.data, changed.size, &unchanged) ==
                MFW_CONTRACT &&
            unchanged == NULL);
    }
    free(changed.data);
    passed("schema-identity-mismatch-refused");
  }
  for (i = 0; i < 2; i++) {
    mfw_state_v1 *s = state(i == 0 ? 0x41 : 0x42, 10, 100, 65536);
    mfw_frame_v1 *unchanged = NULL;
    wire w = native[i == 0 ? 0 : 1];
    CHECK(mfw_state_v1_receive(s, w.data, w.size, &unchanged) ==
              MFW_CORRELATION &&
          unchanged == NULL);
    CHECK(snapshot(s).phase == MFW_UNKNOWN_PHASE);
    mfw_state_v1_free(s);
  }
  passed("epoch-and-sequence-refusals-are-sticky");
  {
    mfw_frame_v1 *f = decode(native[0]);
    uint8_t buffer[MFW_MAX_WIRE_BYTES];
    uint32_t written = 0xdeadbeef;
    uint32_t *overlap_buffer = (uint32_t *)malloc(sizeof(buffer));
    mfw_frame_info_v1 bad_info;
    mfw_frame_info_v1 original_info;
    CHECK(overlap_buffer != NULL);
    memset(buffer, 0xa5, sizeof(buffer));
    CHECK(mfw_frame_v1_encode(f, buffer, native[0].size - 1, &written) ==
              MFW_BUFFER &&
          written == 0xdeadbeef);
    CHECK(all_bytes(buffer, sizeof(buffer), 0xa5));
    memset(overlap_buffer, 0xa5, sizeof(buffer));
    CHECK(mfw_frame_v1_encode(f, (uint8_t *)overlap_buffer, sizeof(buffer),
                              overlap_buffer) == MFW_INVALID);
    CHECK(all_bytes((uint8_t *)overlap_buffer, sizeof(buffer), 0xa5));
    memset(&bad_info, 0xa5, sizeof(bad_info));
    original_info = bad_info;
    CHECK(mfw_frame_v1_info(f, &bad_info, sizeof(bad_info) - 1) ==
              MFW_INVALID &&
          memcmp(&bad_info, &original_info, sizeof(bad_info)) == 0);
    {
      mfw_change_view_v1 c, prior;
      memset(&c, 0xa5, sizeof(c));
      prior = c;
      CHECK(mfw_frame_v1_change(f, 9, &c, sizeof(c)) == MFW_INVALID &&
            memcmp(&c, &prior, sizeof(c)) == 0);
    }
    free(overlap_buffer);
    mfw_frame_v1_free(f);
    passed("output-capacity-overlap-size-and-index-preservation");
  }
  {
    void *buffer = malloc(MFW_MAX_WIRE_BYTES);
    mfw_frame_v1 *f = NULL;
    mfw_state_v1 *s = state(0x42, 10, 100, 65536);
    CHECK(buffer != NULL);
    memcpy(buffer, native[0].data, native[0].size);
    OK(mfw_frame_v1_decode((const uint8_t *)buffer, native[0].size,
                           (mfw_frame_v1 **)buffer));
    memcpy(&f, buffer, sizeof(f));
    roundtrip(f, native[0]);
    mfw_frame_v1_free(f);
    memcpy(buffer, native[0].data, native[0].size);
    OK(mfw_state_v1_receive(s, (const uint8_t *)buffer, native[0].size,
                            (mfw_frame_v1 **)buffer));
    memcpy(&f, buffer, sizeof(f));
    roundtrip(f, native[0]);
    ack(s, native[0], 1);
    mfw_frame_v1_free(f);
    mfw_state_v1_free(s);
    free(buffer);
    passed("copied-input-may-alias-result-slot");
  }
  for (i = 1; i <= 5; i++) {
    char name[64];
    wire w;
    mfw_state_v1 *s = state(0x43, 1, 0, 65536);
    mfw_frame_v1 *f;
    mfw_frame_info_v1 v;
    CHECK(snprintf(name, sizeof(name), "terminal-%u.capnp", i) > 0);
    w = synthetic(argv[7], name);
    f = receive(s, w);
    v = info(f);
    CHECK(v.end == i && v.changes == 0 && v.sequence == 1);
    roundtrip(f, w);
    ack(s, w, 1);
    OK(mfw_state_v1_transport_lost(s));
    CHECK(snapshot(s).phase == MFW_ENDED_PHASE && snapshot(s).end == i);
    mfw_frame_v1_free(f);
    mfw_state_v1_free(s);
    free(w.data);
  }
  passed("five-explicit-synthetic-terminals-not-native-close");
  {
    const uint8_t expected[] = {0, 0xd8, 0x61, 0};
    mfw_frame_v1 *f = decode(unpaired);
    mfw_change_view_v1 c;
    OK(mfw_frame_v1_change(f, 0, &c, sizeof(c)));
    CHECK(c.name_bytes == sizeof(expected) &&
          memcmp(c.name, expected, sizeof(expected)) == 0);
    roundtrip(f, unpaired);
    mfw_frame_v1_free(f);
    passed("unpaired-utf16-surrogate-preserved");
  }
  {
    uint8_t epoch[32] = {0};
    mfw_state_v1 *out = NULL;
    mfw_state_limits_v1 limits = {1, sizeof(limits), 1, 1, 65536, 0};
    CHECK(mfw_state_v1_new(epoch, &limits, sizeof(limits), &out) ==
              MFW_INVALID &&
          out == NULL);
    memset(epoch, 0x42, sizeof(epoch));
    limits.reserved = 1;
    CHECK(mfw_state_v1_new(epoch, &limits, sizeof(limits), &out) ==
              MFW_INVALID &&
          out == NULL);
    limits.reserved = 0;
    limits.abi_version = 2;
    CHECK(mfw_state_v1_new(epoch, &limits, sizeof(limits), &out) ==
              MFW_INVALID &&
          out == NULL);
    limits.abi_version = 1;
    limits.frames = 0;
    CHECK(mfw_state_v1_new(epoch, &limits, sizeof(limits), &out) == MFW_LIMIT &&
          out == NULL);
    mfw_state_v1_free(NULL);
    mfw_frame_v1_free(NULL);
    passed("constructor-contract-and-null-disposal");
  }
  for (i = 0; i < 6; i++) {
    free(native[i].data);
  }
  free(terminal.data);
  free(gap.data);
  free(unpaired.data);
  printf("C_WATCH_CONFORMANCE_PASS scenarios=%u assertions=%u "
         "original_native_payloads=6 synthetic_vectors=7 native_authority=0\n",
         scenarios, checks);
  return 0;
}
