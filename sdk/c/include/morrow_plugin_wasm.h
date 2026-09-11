#ifndef MORROW_PLUGIN_WASM_H
#define MORROW_PLUGIN_WASM_H
#include "morrow_plugin_sdk.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Fixed wasm32 import. Guest offsets never encode host identity or authority.
 */
mp_host_v1 mp_wasm_host(void);
#ifdef __cplusplus
}
#endif
#endif
