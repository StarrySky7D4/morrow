#include "morrow_plugin_service.hpp"
#include <cassert>
#include <cstdlib>
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
  const char *expected=std::getenv("MORROW_SDK_EXPECTED_SERVICE_DIGEST");assert(expected);
  std::string digest_text;const char *hex="0123456789abcdef";
  for(auto b:request.digest()){digest_text+=hex[b>>4];digest_text+=hex[b&15];}
  assert(digest_text==expected);
  auto view=request.view();
  assert(view.call_id==UINT64_MAX && view.header_count>=2 && view.header_count<=4 && view.body.length==3);
  static_assert(!std::is_copy_constructible_v<morrow::service_resources>);
  auto resources=request.resources();
  assert(resources.status()==(view.header_count==4 ? MP_CODEC_INVALID : MP_CODEC_OK));
  const bool present=view.header_count==3;
  assert(resources.present()==present);
  auto resources_moved=std::move(resources);
  assert(!resources.present());
  morrow::service_resources retained; retained=std::move(resources_moved);
  assert(!resources_moved.present());
  mp_service_reply_v1 reply{};
  reply.abi_version=MP_SERVICE_ABI_VERSION; reply.struct_size=sizeof(reply);
  reply.status=200; reply.body=view.body; reply.headers=view.headers; reply.header_count=view.header_count;
  auto frame=request.response(reply);
  assert(frame.status==MP_CODEC_OK);
  request=morrow::service_request{};
  if(present){
    auto rv=retained.view();
    assert(rv.scope_sha256.length==32 && rv.scope_sha256.data[0]==7 && rv.endpoint_count==1);
    assert(rv.endpoints[0].method_count==2 && rv.endpoints[0].methods[1].data[0]=='P');
    assert(rv.endpoints[0].max_request_bytes==1024 && rv.endpoints[0].response_frame_limit==4096);
  }
  assert(std::fwrite(frame.bytes.data(),1,frame.bytes.size(),stdout)==frame.bytes.size());
}
