#include "morrow_channel_v1.hpp"
#include <cassert>
#include <stdexcept>
#include <fstream>
#include <string>
int main(int argc,char **argv){
  auto callback=[](const uint8_t*,uint32_t,uint8_t*,uint32_t,uint32_t*)->uint32_t{throw std::runtime_error("uncertain submission");};
  auto host=morrow::channel_v1::local_adapter<decltype(callback)>::host(callback);
  morrow::channel_v1::request r;r.call_id.fill(1);r.reference.fill(2);r.source_epoch.fill(3);
  auto response=morrow::channel_v1::response::call(host,r);assert(response.code()==MP_TRANSPORT_FAILURE);
  mp_channel_response_view view{};assert(response.view(view)==MP_TRANSPORT_FAILURE);
  assert(argc==2);std::ifstream file(std::string(argv[1])+"/directory.capnp",std::ios::binary);assert(file);
  file.seekg(0,std::ios::end);std::streamoff length=file.tellg();assert(length>0);
  size_t byte_count=static_cast<size_t>(length);assert(byte_count<=MP_MAX_CHANNEL_WIRE_BYTES);file.seekg(0);
  std::vector<uint8_t> encoded(byte_count);file.read(reinterpret_cast<char*>(encoded.data()),static_cast<std::streamsize>(byte_count));assert(file);
  auto directory=morrow::channel_v1::directory::decode(encoded);assert(directory.code()==MP_CODEC_OK);encoded.clear();
  mp_channel_directory_view dv{};assert(directory.view(dv)==MP_CODEC_OK&&dv.channel_count==1&&dv.scope_sha256.data[0]==4);
  assert(dv.channels[0].reference.data[0]==2&&dv.channels[0].source_epoch.data[0]==3&&dv.channels[0].budget.max_frame_bytes==65536);
  return 0;
}
