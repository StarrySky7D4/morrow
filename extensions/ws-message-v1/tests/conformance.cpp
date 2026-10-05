#define MWS_CONFORMANCE_CPP
#include "conformance.c"
#include "morrow_ws_message_v1.hpp"
#include <type_traits>
#include <utility>
namespace ws=morrow::ws_message_v1;
int main(int argc,char **argv){
 CHECK(argc==3);c_corpus(argv[1],argv[2]);extra_c_methods();
 static_assert(!std::is_copy_constructible_v<ws::message>);static_assert(std::is_move_constructible_v<ws::message>);
 FILE *f=fopen(argv[1],"rb"),*out=fopen(argv[2],"wb");CHECK(f&&out);uint8_t magic[8];CHECK(fread(magic,1,8,f)==8);CHECK(fwrite(magic,1,8,out)==8);write32(out,0);uint32_t count=read32(f),accepted=0,encoded_count=0,encode_limits=0;
 ws::message m;std::vector<uint8_t> wire;
 for(uint32_t i=0;i<count;i++){
  auto r=read_record(f);auto previous=m.view();uint32_t rc=m.decode(r.wire,r.wn);CHECK((rc==0)==(r.ok!=0));
  if(r.ok){equal_message(&m.view(),&r);memset(r.wire,0xcc,r.wn);equal_message(&m.view(),&r);wire={0xaa,0xbb,0xcc};auto oldwire=wire;rc=m.encode(wire);if(r.enc){CHECK(rc==0);write_record(out,&r,wire.data(),static_cast<uint32_t>(wire.size()));encoded_count++;}else{CHECK(rc==MWS_V1_LIMIT&&wire==oldwire);encode_limits++;}ws::message other;CHECK(other.assign(static_cast<ws::kind>(r.k),r.payload,r.pn,r.has?std::optional<uint16_t>(static_cast<uint16_t>(r.code)):std::nullopt)==0);auto moved=std::move(other);equal_message(&moved.view(),&r);CHECK(other.decode(nullptr,0)==MWS_V1_INVALID);CHECK(other.encode(wire)==MWS_V1_INVALID);CHECK(moved.assign(static_cast<ws::kind>(r.k),r.payload,r.pn,r.has?std::optional<uint16_t>(static_cast<uint16_t>(r.code)):std::nullopt)==0);equal_message(&moved.view(),&r);auto snapshot=moved.view();CHECK(moved.assign(static_cast<ws::kind>(99),nullptr,0,std::nullopt)!=0);CHECK(memcmp(&snapshot,&moved.view(),sizeof(snapshot))==0);accepted++;}else{CHECK(memcmp(&previous,&m.view(),sizeof(previous))==0);}
  free_record(&r);
 }
 CHECK(fgetc(f)==EOF);CHECK(fclose(f)==0);CHECK(fseek(out,8,SEEK_SET)==0);write32(out,encoded_count);CHECK(fclose(out)==0);
 printf("METHOD cpp_owned_move_only_corpus_and_roundtrips PASS vectors=%u accepted=%u\n",count,accepted);printf("METHOD cpp_assign_and_decode_error_preserves_owned_message PASS\n");printf("METHOD cpp_encode_limit_preserves_vector PASS encode_limits=%u serialized_roundtrips=%u\n",encode_limits,encoded_count);printf("METHOD cpp_moved_from_methods_refuse PASS\n");return 0;
}
