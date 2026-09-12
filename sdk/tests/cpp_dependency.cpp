#include "morrow_plugin_dependency.hpp"
#include <cassert>
#include <fstream>
#include <iterator>
#include <iostream>
int main(int argc,char**argv){
  assert(argc==2);std::ifstream file(argv[1],std::ios::binary);assert(file.good());
  std::vector<uint8_t> bytes{std::istreambuf_iterator<char>(file),{}};
  morrow::dependency_request request("call1","reverse",{'a','b','c'});
  assert(morrow::encode_dependency(request).status==MP_CODEC_OK);
  auto value=morrow::dependency_output::decode(request.encode(),bytes);assert(value.status()==MP_CODEC_OK);
  auto moved=std::move(value);assert(value.status()!=MP_CODEC_OK);
  morrow::dependency_output assigned;assigned=std::move(moved);assert(moved.status()!=MP_CODEC_OK);
  bytes.assign(bytes.size(),0);auto view=assigned.view();
  assert(std::string(reinterpret_cast<const char*>(view.output_type.data),view.output_type.length)=="bytes");
  assert(std::string(reinterpret_cast<const char*>(view.bytes.data),view.bytes.length)=="def");
  assert(morrow::dependency_output::decode(request.encode(),bytes).status()!=MP_CODEC_OK);
  morrow::dependency_request too_big("call1","reverse",std::vector<uint8_t>(65537));
  assert(too_big.encode().status==MP_CODEC_LIMIT && too_big.encode().bytes.empty());
  std::cout<<"PASS_SCOPED C++ dependency ownership move-only output and bounds\n";
}
