#include "morrow_blob_upload_request_v1.hpp"
#include <array>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <type_traits>
namespace upload=morrow::blob_upload_request_v1;
static unsigned checks;
#define CHECK(x) do { ++checks; if (!(x)) { std::fprintf(stderr,"check failed at line %u: %s\n",(unsigned)__LINE__,#x); std::exit(1); } } while (0)
#define OK(x) CHECK((x)==MBU_OK)
static_assert(!std::is_copy_constructible_v<upload::request> && !std::is_copy_assignable_v<upload::request>);
static_assert(!std::is_copy_constructible_v<upload::response> && !std::is_copy_assignable_v<upload::response>);
static_assert(!std::is_copy_constructible_v<upload::client> && !std::is_copy_assignable_v<upload::client>);
static_assert(std::is_nothrow_move_constructible_v<upload::client> && std::is_nothrow_move_assignable_v<upload::client>);
static mbu_client_options options() {
    mbu_client_options o{}; o.abi_version=1; o.struct_size=sizeof(o); o.max_calls=16; o.max_wire_bytes=65536;
    o.descriptor.abi_version=1; o.descriptor.struct_size=sizeof(o.descriptor);
    std::memset(o.nomination_ref,1,32); std::memset(o.descriptor.transfer_epoch,2,32);
    std::memset(o.descriptor.object_ref,3,32); std::memset(o.descriptor.operation_id,4,32);
    constexpr std::array<uint8_t,32> empty={0xe3,0xb0,0xc4,0x42,0x98,0xfc,0x1c,0x14,0x9a,0xfb,0xf4,0xc8,0x99,0x6f,0xb9,0x24,0x27,0xae,0x41,0xe4,0x64,0x9b,0x93,0x4c,0xa4,0x95,0x99,0x1b,0x78,0x52,0xb8,0x55};
    std::memcpy(o.descriptor.sha256,empty.data(),32); return o;
}
static mbu_request_view view(uint8_t nonce) {
    mbu_request_view v{}; v.abi_version=1; v.struct_size=sizeof(v); v.action=MBU_OPEN;
    std::memset(v.nomination_ref,1,32); v.nonce[0]=nonce; return v;
}
struct callback_context { unsigned calls=0; bool fail=false; };
static int32_t callback(void *raw, const uint8_t *bytes, uint32_t n, uint8_t *out, uint32_t cap) noexcept {
    auto& c=*static_cast<callback_context*>(raw); ++c.calls;
    if (c.fail) return -1;
    mbu_request *q=nullptr; mbu_response *r=nullptr; uint32_t length=0;
    if (mbu_request_decode(bytes,n,&q)!=MBU_OK) return -1;
    mbu_reply_view v{}; v.abi_version=1; v.struct_size=sizeof(v); v.kind=MBU_REPLY_STATE; v.phase=MBU_READY;
    v.descriptor=options().descriptor; v.next_sequence=1;
    const auto made=mbu_response_new(q,&v,sizeof(v),&r);
    const auto encoded=made==MBU_OK ? mbu_response_encode(r,out,cap,&length) : MBU_STATE;
    mbu_response_free(r); mbu_request_free(q);
    return encoded==MBU_OK ? static_cast<int32_t>(length) : -1;
}
int main() {
    auto o=options(); auto v=view(1); upload::request q; OK(q.assign(v));
    std::vector<uint8_t> original; OK(q.encode(original));
    auto invalid=v; invalid.reserved=1; CHECK(q.assign(invalid)==MBU_INVALID);
    std::vector<uint8_t> still; OK(q.encode(still)); CHECK(still==original);
    upload::request moved(std::move(q)); std::vector<uint8_t> canary{1,2,3};
    CHECK(q.encode(canary)==MBU_BUFFER); CHECK((canary==std::vector<uint8_t>{1,2,3}));
    q=std::move(moved); CHECK(moved.native()==nullptr); CHECK(q.native()!=nullptr);
    upload::request same; OK(same.decode(original.data(),static_cast<uint32_t>(original.size())));
    std::vector<uint8_t> round; OK(same.encode(round)); CHECK(round==original);
    upload::response response; mbu_reply_view rv{};
    rv.abi_version=1; rv.struct_size=sizeof(rv); rv.kind=MBU_REPLY_STATE; rv.phase=MBU_READY;
    rv.descriptor=o.descriptor; rv.next_sequence=1; OK(response.assign(q,rv));
    std::vector<uint8_t> reply; OK(response.encode(reply));
    upload::request other; OK(other.assign(view(2))); upload::response decoded;
    CHECK(decoded.decode_for(reply.data(),static_cast<uint32_t>(reply.size()),other)==MBU_CORRELATION);
    OK(decoded.decode_for(reply.data(),static_cast<uint32_t>(reply.size()),q));
    upload::response moved_response(std::move(decoded)); CHECK(decoded.encode(canary)==MBU_BUFFER);
    CHECK((canary==std::vector<uint8_t>{1,2,3})); decoded=std::move(moved_response);
    mbu_reply_view got{}; OK(decoded.view(got)); CHECK(got.phase==MBU_READY);
    upload::client c; OK(c.initialize(o)); CHECK(c.initialize(o)==MBU_STATE);
    callback_context context; upload::response delivered;
    OK(c.call_once(v,callback,&context,delivered)); CHECK(context.calls==1);
    mbu_snapshot before{}, after{}; OK(c.snapshot(before)); CHECK(before.phase==MBU_CLIENT_READY && before.calls==1 && !before.pending);
    upload::client moved_client(std::move(c)); CHECK(c.snapshot(after)==MBU_BUFFER);
    OK(moved_client.snapshot(after)); CHECK(after.wire_bytes==before.wire_bytes && after.calls==before.calls);
    OK(moved_client.abandon()); OK(moved_client.snapshot(after)); CHECK(after.phase==MBU_CLIENT_CLOSED);
    CHECK(moved_client.initialize(o)==MBU_STATE);
    upload::client uncertain; OK(uncertain.initialize(o)); context={0,true};
    const auto old_response=delivered.native();
    CHECK(uncertain.call_once(v,callback,&context,delivered)==MBU_STATE); CHECK(delivered.native()==old_response);
    OK(uncertain.snapshot(before)); CHECK(before.phase==MBU_CLIENT_UNKNOWN && before.pending && before.calls==1);
    CHECK(uncertain.initialize(o)==MBU_STATE);
    CHECK(uncertain.call_once(view(3),callback,&context,delivered)==MBU_STATE);
    OK(uncertain.abandon()); OK(uncertain.snapshot(after));
    CHECK(context.calls==1 && after.wire_bytes==before.wire_bytes && after.pending && after.phase==MBU_CLIENT_UNKNOWN);
    std::printf("{\"status\":\"PASS_SCOPED_NATIVE_CPP\",\"checks\":%u,\"ownership_moves\":true,\"callback_replay\":false,\"store_proof\":false}\n",checks);
    return 0;
}
