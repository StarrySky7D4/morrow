#include "morrow_fs_watch_v1.h"
#include <stdlib.h>
#include <string.h>
_Static_assert(sizeof(void *) == 4, "Wasm32 guest");
_Static_assert(sizeof(mfw_frame_info_v1) == 64, "frame ABI");
_Static_assert(sizeof(mfw_change_view_v1) == 20, "change ABI");
_Static_assert(sizeof(mfw_state_snapshot_v1) == 40, "state ABI");
__attribute__((import_module("morrow_task_v1"),
               import_name("read_input"))) int32_t
task_read(uint8_t *, uint32_t);
__attribute__((import_module("morrow_task_v1"),
               import_name("complete"))) int32_t
task_complete(const uint8_t *, uint32_t);
static int state_check(const uint8_t *input, uint32_t length,
                       const mfw_frame_info_v1 *info) {
  mfw_state_limits_v1 limits = {1, sizeof(limits), 2, 512, 131072, 0};
  mfw_state_v1 *state = NULL;
  mfw_frame_v1 *frame = NULL;
  mfw_frame_v1 *second = NULL;
  mfw_state_snapshot_v1 value;
  uint8_t digest[32];
  int result = 0;
  if (mfw_state_v1_new(info->epoch, &limits, sizeof(limits), &state) != MFW_OK)
    goto done;
  if (mfw_state_v1_receive(state, input, length, &frame) != MFW_OK ||
      mfw_wire_v1_digest(input, length, digest, 32) != MFW_OK)
    goto done;
  if (mfw_state_v1_snapshot(state, &value, sizeof(value)) != MFW_OK ||
      value.phase != MFW_PENDING_PHASE || value.frames != 1 ||
      value.wire_bytes != length)
    goto done;
  if (mfw_state_v1_receive(state, input, length, &second) != MFW_PENDING ||
      second != NULL || mfw_state_v1_ack(state, 2, digest) != MFW_CORRELATION)
    goto done;
  if (mfw_state_v1_ack(state, 1, digest) != MFW_OK ||
      mfw_state_v1_transport_lost(state) != MFW_OK ||
      mfw_state_v1_snapshot(state, &value, sizeof(value)) != MFW_OK)
    goto done;
  if (value.phase !=
          (info->end == MFW_CONTINUE ? MFW_UNKNOWN_PHASE : MFW_ENDED_PHASE) ||
      value.frames != 1 || value.wire_bytes != length)
    goto done;
  if (mfw_state_v1_ack(state, 1, digest) !=
      (info->end == MFW_CONTINUE ? MFW_UNKNOWN : MFW_STATE))
    goto done;
  result = 1;
done:
  mfw_frame_v1_free(second);
  mfw_frame_v1_free(frame);
  mfw_state_v1_free(state);
  return result;
}
int32_t morrow_run(void) {
  uint8_t *input = (uint8_t *)malloc(131072);
  uint8_t *output = (uint8_t *)malloc(MFW_MAX_WIRE_BYTES);
  mfw_frame_v1 *frame = NULL;
  mfw_frame_info_v1 info;
  uint32_t written = 0, i;
  int32_t result = -1, length;
  if (input == NULL || output == NULL)
    goto done;
  length = task_read(input, 131072);
  if (length <= 0 || (uint32_t)length > MFW_MAX_WIRE_BYTES)
    goto done;
  if (mfw_frame_v1_decode(input, (uint32_t)length, &frame) != MFW_OK ||
      mfw_frame_v1_info(frame, &info, sizeof(info)) != MFW_OK)
    goto done;
  for (i = 0; i < info.changes; i++) {
    mfw_change_view_v1 view;
    if (mfw_frame_v1_change(frame, i, &view, sizeof(view)) != MFW_OK ||
        view.name_bytes == 0 || view.name_bytes % 2 != 0)
      goto done;
  }
  if (mfw_frame_v1_encode(frame, output, MFW_MAX_WIRE_BYTES, &written) !=
          MFW_OK ||
      written != (uint32_t)length || memcmp(output, input, written) != 0)
    goto done;
  if (info.sequence == 1 && !state_check(input, (uint32_t)length, &info))
    goto done;
  result = task_complete(output, written);
done:
  mfw_frame_v1_free(frame);
  free(output);
  free(input);
  return result;
}
