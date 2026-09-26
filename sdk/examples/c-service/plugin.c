/* Host supplies an authenticated, bounded service request. No socket imports. */
#include "morrow_plugin_service.h"
int32_t morrow_run(void) {
  mp_service_request *request=NULL;
  mp_service_request_view view={0};
  mp_service_reply_v1 reply={0};
  uint32_t status=mp_wasm_service_read(&request);
  if (status==MP_CODEC_OK) status=mp_service_request_get(request,&view,sizeof(view));
  if (status==MP_CODEC_OK) {
    reply.abi_version=MP_SERVICE_ABI_VERSION; reply.struct_size=sizeof(reply);
    reply.status=200; reply.body=view.body;
    status=mp_wasm_service_complete(request,&reply);
  }
  mp_service_request_free(request);
  return status==MP_CODEC_OK?0:-1;
}
