#include "morrow_plugin_service.hpp"
extern "C" int32_t morrow_run() {
  auto request=morrow::service_request::read();
  if(request.status()!=MP_CODEC_OK)return -1;
  auto view=request.view();
  mp_service_reply_v1 reply{};
  reply.abi_version=MP_SERVICE_ABI_VERSION; reply.struct_size=sizeof(reply);
  reply.status=200; reply.body=view.body;
  return request.complete(reply)==MP_CODEC_OK?0:-1;
}
