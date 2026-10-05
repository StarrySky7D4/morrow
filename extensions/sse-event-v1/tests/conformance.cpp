#define MSE_CONFORMANCE_CPP
#include "conformance.c"
#include "morrow_sse_event_v1.hpp"
#include <type_traits>
#include <utility>
#include <string>
namespace sse=morrow::sse_event_v1;
int main(int argc,char **argv){
 CHECK(argc==3);c_corpus(argv[1],argv[2]);c_extra_methods();static_assert(!std::is_copy_constructible_v<sse::event>);static_assert(std::is_move_constructible_v<sse::event>);
 FILE *f=fopen(argv[1],"rb"),*out=fopen(argv[2],"wb");CHECK(f&&out);uint8_t magic[8];CHECK(fread(magic,1,8,f)==8);CHECK(fwrite(magic,1,8,out)==8);write32(out,0);uint32_t count=read32(f),accept=0,encoded_count=0,limits=0;sse::event value;std::vector<uint8_t> wire;
 for(uint32_t idx=0;idx<count;idx++){
  auto r=read_record(f);auto before=value.view();uint32_t rc=value.decode(r.w,r.wn);CHECK((rc==0)==(r.ok!=0));
  if(r.ok){equal_event(&value.view(),&r);memset(r.w,0xcc,r.wn);equal_event(&value.view(),&r);wire={0xaa,0xbb,0xcc};auto oldwire=wire;rc=value.encode(wire);if(r.enc){CHECK(rc==0);write_record(out,&r,wire.data(),static_cast<uint32_t>(wire.size()));encoded_count++;}else{CHECK(rc==MSE_V1_LIMIT&&wire==oldwire);limits++;}sse::event other;CHECK(other.assign(r.d,r.dn,r.e,r.en,r.id,r.in,r.has?std::optional<uint64_t>(r.retry):std::nullopt)==0);equal_event(&other.view(),&r);auto moved=std::move(other);equal_event(&moved.view(),&r);auto snapshot=moved.view();CHECK(other.decode(nullptr,0)==MSE_V1_INVALID);auto wire_before_move_refusal=wire;CHECK(other.encode(wire)==MSE_V1_INVALID&&wire==wire_before_move_refusal);const uint8_t bad[]={255};CHECK(moved.assign(bad,1,nullptr,0,nullptr,0)==MSE_V1_UTF8);CHECK(memcmp(&snapshot,&moved.view(),sizeof(snapshot))==0);std::string_view d(reinterpret_cast<const char*>(r.d),r.dn),e(reinterpret_cast<const char*>(r.e),r.en),id(reinterpret_cast<const char*>(r.id),r.in);CHECK(moved.assign(d,e,id,r.has?std::optional<uint64_t>(r.retry):std::nullopt)==0);equal_event(&moved.view(),&r);accept++;}else{CHECK(memcmp(&before,&value.view(),sizeof(before))==0);}
  release_record(&r);
 }
 CHECK(fgetc(f)==EOF);CHECK(fclose(f)==0);CHECK(fseek(out,8,SEEK_SET)==0);write32(out,encoded_count);CHECK(fclose(out)==0);
 auto snapshot=value.view();std::string large(32768,'x');CHECK(value.assign(large,"x",large)==MSE_V1_LIMIT);CHECK(memcmp(&snapshot,&value.view(),sizeof(snapshot))==0);
 printf("METHOD cpp_sse_move_owned_complete_metadata_corpus PASS vectors=%u accepted=%u\n",count,accept);printf("METHOD cpp_sse_decode_assign_errors_preserve_owner PASS\n");printf("METHOD cpp_sse_encode_limit_preserves_vector PASS encoded=%u encode_limits=%u\n",encoded_count,limits);printf("METHOD cpp_sse_moved_from_refusals_preserve_vector PASS\n");printf("METHOD cpp_sse_string_view_aggregate_limit PASS\n");return 0;
}
