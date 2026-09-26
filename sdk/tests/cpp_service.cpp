#include "morrow_plugin_service.hpp"
#include <cassert>
#include <cstdio>
#include <type_traits>
int main() {
  static_assert(!std::is_copy_constructible_v<morrow::service_request>);
  std::vector<uint8_t> input(MP_MAX_SERVICE_FRAME_BYTES);
  size_t count=std::fread(input.data(),1,input.size(),stdin);
  assert(count>0 && !std::ferror(stdin)); input.resize(count);
  auto original=morrow::service_request::decode(input);
  assert(original.status()==MP_CODEC_OK);
  input.assign(input.size(),0);
  auto moved=std::move(original);
  assert(original.status()==MP_CODEC_INVALID);
  morrow::service_request request; request=std::move(moved);
  assert(moved.status()==MP_CODEC_INVALID);
  auto view=request.view();
  assert(view.call_id==UINT64_MAX && view.header_count==2 && view.body.length==3);
  mp_service_reply_v1 reply{};
  reply.abi_version=MP_SERVICE_ABI_VERSION; reply.struct_size=sizeof(reply);
  reply.status=200; reply.body=view.body; reply.headers=view.headers; reply.header_count=view.header_count;
  auto frame=request.response(reply);
  assert(frame.status==MP_CODEC_OK);
  assert(std::fwrite(frame.bytes.data(),1,frame.bytes.size(),stdout)==frame.bytes.size());
}
