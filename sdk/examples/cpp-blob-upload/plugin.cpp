#include "../blob-upload-common/sample.h"
#include "morrow_blob_upload_request_v1.hpp"
#include <array>
#include <algorithm>
using morrow::blob_upload_request_v1::client;
using morrow::blob_upload_request_v1::response;
extern "C" int32_t morrow_run() {
    mbu_client_options options{};
    int release = 0;
    sample_content content{};
    int input = sample_input(&options, &release, &content);
    if (input == 2) return 0;
    if (input) return 1;
    client original;
    if (original.initialize(options) != MBU_OK) return 1;
    client sender(std::move(original));
    response last;
    uint64_t calls = 0;
    auto invoke = [&](uint32_t action, const uint8_t *frame = nullptr, uint32_t size = 0) {
        auto request = sample_request(&options, ++calls, action, frame, size);
        response next;
        auto code = sender.call_once(request, mbu_wasm_transport, nullptr, next);
        if (code == MBU_OK) last = std::move(next);
        return code == MBU_OK;
    };
    mbu_reply_view reply{};
    if (!invoke(MBU_OPEN) || last.view(reply) != MBU_OK || reply.kind != MBU_REPLY_STATE) return 1;
    if (reply.phase == MBU_READY && !release) {
        std::array<uint8_t, MBT_MAX_FRAME_BYTES> wire{};
        std::array<uint8_t, MBT_MAX_CHUNK_BYTES> payload{};
        payload.fill(content.fill);
        uint32_t size = 0;
        if (sample_frame(&options.descriptor, MBT_DESCRIPTOR, 0, 0, nullptr, 0, wire.data(), &size) ||
            !invoke(MBU_PUSH, wire.data(), size)) return 1;
        uint64_t sequence = 1;
        for (uint64_t offset = 0; offset < options.descriptor.byte_length; ++sequence) {
            const auto length = static_cast<uint32_t>(std::min<uint64_t>(payload.size(), options.descriptor.byte_length - offset));
            const uint8_t *bytes = payload.data();
            if (content.is_text) {
                if (offset + length > content.text_length) return 1;
                bytes = content.text + static_cast<uint32_t>(offset);
            }
            if (sample_frame(&options.descriptor, MBT_CHUNK, sequence, offset, bytes, length, wire.data(), &size) ||
                !invoke(MBU_PUSH, wire.data(), size)) return 1;
            offset += length;
        }
        if (sample_frame(&options.descriptor, MBT_END, 0, 0, nullptr, 0, wire.data(), &size) ||
            !invoke(MBU_PUSH, wire.data(), size)) return 1;
    }
    if (!invoke(release ? MBU_RELEASE : MBU_INSPECT) || last.view(reply) != MBU_OK || reply.kind != MBU_REPLY_STATE ||
        reply.phase != (release ? MBU_RELEASED : MBU_STAGED)) return 1;
    return mbu_wasm_complete_response(last.native()) == MBU_OK ? 0 : 1;
}
