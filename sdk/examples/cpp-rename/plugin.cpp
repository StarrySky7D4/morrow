#include "morrow_plugin_codec.hpp"
#include "morrow_plugin_sdk.hpp"
#include "morrow_plugin_wasm.h"
extern "C" int32_t morrow_run() {
  auto request =
      morrow::request::rename("wasm-op", "legacy-123", 1, "Wasm SDK rename");
  auto input = request.encode();
  if (input.status != MP_CODEC_OK)
    return 99;
  morrow::client client(mp_wasm_host());
  auto response = client.exchange(input.bytes.data(), input.bytes.size());
  if (response.status != MP_OK)
    return -1;
  auto decoded = morrow::decoded_reply::decode(request, response.bytes);
  if (decoded.status() != MP_CODEC_OK)
    return 99;
  auto view = decoded.view();
  if (view.kind == MP_REPLY_REJECTED && view.failure == 0)
    return 10;
  if (view.kind == MP_REPLY_RENAMED && view.revision == 2)
    return 20;
  return 99;
}
