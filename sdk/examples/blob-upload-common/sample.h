#ifndef MORROW_UPLOAD_SAMPLE_H
#define MORROW_UPLOAD_SAMPLE_H
#include "morrow_plugin_task.h"
#include "morrow_blob_transfer_v1.h"
#include "morrow_blob_upload_wasm.h"
#include "morrow_blob_upload_proposal_v1.h"
#include "sample_options.h"
#include <string.h>
#ifdef __cplusplus
#include "morrow_blob_upload_proposal_v1.hpp"
#endif
#ifdef __cplusplus
#define SAMPLE_ZERO {}
extern "C" {
#else
#define SAMPLE_ZERO {0}
#endif
/* The aggregate guest codec archive provides bounded hashing, with no imports.
 * The caller supplies live input and writable 32-byte output. */
uint32_t mbu_guest_chunk_sha256(const uint8_t *, uint32_t, uint8_t *);
#ifdef __cplusplus
}
#endif
static int sample_equal(mp_span span, const char *text) {
    size_t size = strlen(text);
    return span.length == size && !memcmp(span.data, text, size);
}
typedef struct sample_content {
    uint8_t fill, text[MBU_SAMPLE_MAX_TEXT_BYTES];
    uint32_t text_length;
    int is_text;
} sample_content;
static void sample_options_fill(mbu_client_options *options, uint64_t length,
                                const uint8_t *sha, const uint8_t *epoch,
                                const uint8_t *object, const uint8_t *operation) {
    memset(options, 0, sizeof(*options));
    options->abi_version = MBU_VERSION; options->struct_size = sizeof(*options);
    options->max_calls = 1024; options->max_wire_bytes = 64u * 1024u * 1024u;
    options->descriptor.abi_version = MBU_VERSION;
    options->descriptor.struct_size = sizeof(options->descriptor);
    options->descriptor.byte_length = length;
    /* In this example the host nominates exactly the fresh transfer epoch. */
    memcpy(options->nomination_ref, epoch, 32);
    memcpy(options->descriptor.transfer_epoch, epoch, 32);
    memcpy(options->descriptor.object_ref, object, 32);
    memcpy(options->descriptor.operation_id, operation, 32);
    memcpy(options->descriptor.sha256, sha, 32);
}
static int sample_emit_plan(const mp_task *task, const uint8_t *bytes, uint32_t length) {
    uint8_t completion[MP_MAX_TASK_BYTES]; uint32_t written = 0;
    if (mp_task_output(task, bytes, length, completion, sizeof(completion), &written) != 0) return 1;
    return mp_wasm_task_complete(completion, written) == 0 ? 0 : 1;
}
static int sample_plan(const mp_task *task, const mp_transform_view *transform, int text) {
    mbu_sample_options input = SAMPLE_ZERO;
    mbp_proposal_view view = SAMPLE_ZERO;
    if (!sample_equal(transform->output_type, MBP_OUTPUT_TYPE)) return 1;
    view.abi_version = MBP_VERSION; view.struct_size = sizeof(view);
    view.producer_input = transform->input.data; view.producer_input_length = transform->input.length;
    if (text) {
        if (!sample_equal(transform->input_type, MBU_SAMPLE_TEXT_TYPE) ||
            mbu_sample_text_sha256(view.producer_input, view.producer_input_length, view.whole_sha256) != 0) return 1;
        view.byte_length = transform->input.length;
        view.label = (const uint8_t*)MBU_SAMPLE_TEXT_LABEL; view.label_length = sizeof(MBU_SAMPLE_TEXT_LABEL)-1;
    } else {
        if (!sample_equal(transform->input_type, MBU_SAMPLE_OPTIONS_TYPE) ||
            mbu_sample_options_decode(transform->input.data, transform->input.length, &input, sizeof(input)) != 0) return 1;
        view.byte_length = input.byte_length;
        view.label = (const uint8_t*)MBU_SAMPLE_LABEL; view.label_length = sizeof(MBU_SAMPLE_LABEL)-1;
        if (mbu_sample_repeated_sha256(input.byte_length, input.fill, view.whole_sha256) != 0) return 1;
    }
#ifdef __cplusplus
    morrow::blob_upload_proposal_v1::proposal proposal;
    if (proposal.assign(view) != MBP_OK) return 1;
    std::vector<uint8_t> wire;
    if (proposal.encode(wire) != MBP_OK) return 1;
    return sample_emit_plan(task, wire.data(), static_cast<uint32_t>(wire.size()));
#else
    mbp_proposal *proposal = NULL; uint8_t wire[MBP_MAX_PROPOSAL_BYTES]; uint32_t length = 0;
    if (mbp_proposal_new(&view, sizeof(view), &proposal) != MBP_OK) return 1;
    uint32_t code = mbp_proposal_encode(proposal, wire, sizeof(wire), &length);
    mbp_proposal_free(proposal);
    return code == MBP_OK ? sample_emit_plan(task, wire, length) : 1;
#endif
}
static int sample_bound_options(mbu_client_options *options, sample_content *content,
                                 const mbp_proposal_view *view, const mbp_identity *identity, int text) {
    mbu_sample_options input = SAMPLE_ZERO;
    if (text) {
        uint8_t hash[32];
        if (view->producer_input_length != view->byte_length ||
            mbu_sample_text_sha256(view->producer_input, view->producer_input_length, hash) != 0 ||
            memcmp(hash, view->whole_sha256, sizeof(hash))) return 1;
        if (view->producer_input_length) memcpy(content->text, view->producer_input, view->producer_input_length);
        content->text_length = view->producer_input_length; content->is_text = 1;
    } else {
        if (mbu_sample_options_decode(view->producer_input, view->producer_input_length, &input, sizeof(input)) != 0 ||
            input.byte_length != view->byte_length) return 1;
        content->fill = input.fill;
    }
    sample_options_fill(options, view->byte_length, view->whole_sha256, identity->transfer_epoch,
                        identity->object_ref, identity->operation_id);
    return 0;
}
static int sample_bound_input(mp_span bytes, mbu_client_options *options, sample_content *content, int text) {
    mbp_proposal_view view = SAMPLE_ZERO; mbp_identity identity = SAMPLE_ZERO;
#ifdef __cplusplus
    morrow::blob_upload_proposal_v1::bound_input input;
    if (input.decode(bytes.data, bytes.length) != MBP_OK || input.view(view) != MBP_OK || input.identity(identity) != MBP_OK) return 1;
    return sample_bound_options(options, content, &view, &identity, text);
#else
    mbp_bound_input *input = NULL; int result = 1;
    if (mbp_bound_decode(bytes.data, bytes.length, &input) != MBP_OK) return 1;
    if (mbp_bound_proposal_view(input, &view, sizeof(view)) == MBP_OK &&
        mbp_bound_identity(input, &identity, sizeof(identity)) == MBP_OK)
        result = sample_bound_options(options, content, &view, &identity, text);
    mbp_bound_free(input); return result;
#endif
}
/* Returns2 only after completing a pure plan;0 configures an approved upload. */
static int sample_input(mbu_client_options *options, int *release, sample_content *content) {
    uint8_t bytes[MP_MAX_TASK_BYTES];
    int32_t size = mp_wasm_task_read(bytes, sizeof(bytes));
    mp_task *task = NULL;
    mbt_frame *frame = NULL;
    mp_transform_view transform = SAMPLE_ZERO;
    mbt_view view = SAMPLE_ZERO;
    int result = 1, text = 0;
    memset(content, 0, sizeof(*content));
    if (size <= 0 || (uint32_t)size > sizeof(bytes) ||
        mp_task_decode(bytes, (uint32_t)size, &task) != 0) goto done;
    if (mp_task_get_transform(task, &transform, sizeof(transform)) != 0) goto done;
    if (sample_equal(transform.handler, "blob.plan")) {
        result = sample_plan(task, &transform, 0) == 0 ? 2 : 1; goto done;
    }
    if (sample_equal(transform.handler, "blob.plan.text")) {
        result = sample_plan(task, &transform, 1) == 0 ? 2 : 1; goto done;
    }
    if (!sample_equal(transform.output_type, "upload.result.v1")) goto done;
    if (sample_equal(transform.handler, "blob.upload")) *release = 0;
    else if (sample_equal(transform.handler, "blob.upload.release")) *release = 1;
    else if (sample_equal(transform.handler, "blob.upload.text")) { *release = 0; text = 1; }
    else if (sample_equal(transform.handler, "blob.upload.text.release")) { *release = 1; text = 1; }
    else goto done;
    if (sample_equal(transform.input_type, MBP_BOUND_INPUT_TYPE)) {
        result = sample_bound_input(transform.input, options, content, text); goto done;
    }
    if (text) goto done;
    if (!sample_equal(transform.input_type, "upload.descriptor.v1")) goto done;
    if (mbt_frame_decode(transform.input.data, transform.input.length, &frame) != MBT_OK ||
        mbt_frame_view(frame, &view, sizeof(view)) != MBT_OK || view.kind != MBT_DESCRIPTOR) goto done;
    content->fill = 42;
    sample_options_fill(options, view.total_length, view.whole_sha256, view.transfer_epoch, view.object_ref, view.operation_id);
    result = 0;
done:
    mbt_frame_free(frame);
    mp_task_free(task);
    return result;
}
static int sample_frame(const mbu_descriptor *descriptor, uint32_t kind, uint64_t sequence,
                        uint64_t offset, const uint8_t *payload, uint32_t size,
                        uint8_t *output, uint32_t *written) {
    mbt_view view = SAMPLE_ZERO;
    mbt_frame *frame = NULL;
    view.abi_version = MBT_VERSION; view.struct_size = sizeof(view); view.kind = kind;
    memcpy(view.transfer_epoch, descriptor->transfer_epoch, 32);
    memcpy(view.object_ref, descriptor->object_ref, 32);
    memcpy(view.operation_id, descriptor->operation_id, 32);
    if (kind == MBT_CHUNK) {
        view.sequence = sequence; view.offset = offset;
        view.payload = payload; view.payload_length = size;
        if (mbu_guest_chunk_sha256(payload, size, view.chunk_sha256) != 0) return 1;
    } else {
        view.total_length = descriptor->byte_length;
        memcpy(view.whole_sha256, descriptor->sha256, 32);
    }
    uint32_t code = mbt_frame_set(&view, sizeof(view), &frame);
    if (code == MBT_OK) code = mbt_frame_encode(frame, output, MBT_MAX_FRAME_BYTES, written);
    mbt_frame_free(frame);
    return code != MBT_OK;
}
static mbu_request_view sample_request(const mbu_client_options *options, uint64_t number,
                                        uint32_t action, const uint8_t *frame, uint32_t length) {
    mbu_request_view request = SAMPLE_ZERO;
    request.abi_version = MBU_VERSION; request.struct_size = sizeof(request); request.action = action;
    memcpy(request.nomination_ref, options->nomination_ref, 32);
    memset(request.nonce, 29, 32);
    for (unsigned i = 0; i < 8; ++i) request.nonce[i] = (uint8_t)(number >> (8 * i));
    request.frame = frame; request.frame_length = length;
    return request;
}
#endif
