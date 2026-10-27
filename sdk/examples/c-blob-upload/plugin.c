#include "../blob-upload-common/sample.h"
static int invoke(mbu_client *client, const mbu_client_options *options, uint64_t *number,
                  uint32_t action, const uint8_t *frame, uint32_t length, mbu_response **last) {
    mbu_request_view request = sample_request(options, ++*number, action, frame, length);
    mbu_response *response = NULL;
    uint32_t code = mbu_client_call_once(client, &request, sizeof(request), mbu_wasm_transport, NULL, &response);
    if (code != MBU_OK) return 1;
    mbu_response_free(*last); *last = response;
    return 0;
}
int32_t morrow_run(void) {
    mbu_client_options options = {0};
    mbu_client *client = NULL;
    mbu_response *response = NULL;
    mbu_reply_view reply = {0};
    uint8_t wire[MBT_MAX_FRAME_BYTES], payload[MBT_MAX_CHUNK_BYTES];
    uint32_t written = 0;
    uint64_t calls = 0, offset = 0, sequence = 1;
    int release = 0, result = 1;
    sample_content content = SAMPLE_ZERO;
    int input = sample_input(&options, &release, &content);
    if (input == 2) return 0;
    if (input || mbu_client_new(&options, sizeof(options), &client) != MBU_OK) goto done;
    if (invoke(client, &options, &calls, MBU_OPEN, NULL, 0, &response) ||
        mbu_response_view_get(response, &reply, sizeof(reply)) != MBU_OK || reply.kind != MBU_REPLY_STATE) goto done;
    if (reply.phase == MBU_READY && !release) {
        if (sample_frame(&options.descriptor, MBT_DESCRIPTOR, 0, 0, NULL, 0, wire, &written) ||
            invoke(client, &options, &calls, MBU_PUSH, wire, written, &response)) goto done;
        memset(payload, content.fill, sizeof(payload));
        while (offset < options.descriptor.byte_length) {
            uint64_t left = options.descriptor.byte_length - offset;
            uint32_t size = (uint32_t)(left < sizeof(payload) ? left : sizeof(payload));
            const uint8_t *bytes = payload;
            if (content.is_text) {
                if (offset + size > content.text_length) goto done;
                bytes = content.text + (uint32_t)offset;
            }
            if (sample_frame(&options.descriptor, MBT_CHUNK, sequence, offset, bytes, size, wire, &written) ||
                invoke(client, &options, &calls, MBU_PUSH, wire, written, &response)) goto done;
            offset += size; ++sequence;
        }
        if (sample_frame(&options.descriptor, MBT_END, 0, 0, NULL, 0, wire, &written) ||
            invoke(client, &options, &calls, MBU_PUSH, wire, written, &response)) goto done;
    }
    if (invoke(client, &options, &calls, release ? MBU_RELEASE : MBU_INSPECT, NULL, 0, &response) ||
        mbu_response_view_get(response, &reply, sizeof(reply)) != MBU_OK || reply.kind != MBU_REPLY_STATE ||
        reply.phase != (release ? MBU_RELEASED : MBU_STAGED)) goto done;
    result = mbu_wasm_complete_response(response) == MBU_OK ? 0 : 1;
done:
    mbu_response_free(response);
    mbu_client_free(client);
    return result;
}
