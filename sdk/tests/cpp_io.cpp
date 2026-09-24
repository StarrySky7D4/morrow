#include "morrow_plugin_io.hpp"
#include <array>
#include <cassert>
#include <fstream>
#include <iostream>
#include <iterator>
static std::vector<uint8_t> file(const char *path){
  std::ifstream input(path,std::ios::binary);assert(input.good());
  return {std::istreambuf_iterator<char>(input),{}};
}
int main(int argc,char **argv){
  assert(argc==3);
  std::array<uint8_t,32> reference;reference.fill(7);
  auto request=morrow::io_request::file_read(77,{'o','p','e','r','a','t','i','o','n','-','1'},reference,1000);
  auto frame=request.encode();assert(frame.status==MP_CODEC_OK && frame.bytes==file(argv[1]));
  auto encoded=file(argv[2]);auto response=morrow::io_response::decode(frame,encoded);
  assert(response.status()==MP_CODEC_OK);
  auto moved=std::move(response);assert(response.status()!=MP_CODEC_OK);
  morrow::io_response assigned;assigned=std::move(moved);assert(moved.status()!=MP_CODEC_OK);
  encoded.assign(encoded.size(),0);
  auto view=assigned.view();assert(view.status==MP_IO_STATUS_OUTCOME_UNKNOWN);
  assert(view.encoded_frame.length==file(argv[2]).size());
  assert(view.bytes.length==0);
  auto wrong=morrow::io_request::file_read(78,{'o','p','e','r','a','t','i','o','n','-','1'},reference,1000).encode();
  assert(morrow::io_response::decode(wrong,file(argv[2])).status()==MP_CODEC_CORRELATION);
  auto invalid=morrow::io_request::query_operation(79,std::vector<uint8_t>(257,'x')).encode();
  assert(invalid.status==MP_CODEC_LIMIT && invalid.bytes.empty());
  std::cout<<"PASS_SCOPED C++ IO move-only response, exact frame and bounds\n";
}
