#include "morrow_plugin_codec.hpp"
extern "C" int32_t morrow_run() {
  morrow::decoded_reply missing;
  (void)missing
      .view(); // Invalid access must trap locally in the no-exceptions profile.
  return 99;
}
