#include "morrow_blob_upload_proposal_v1.hpp"
#include <array>
#include <cassert>
#include <cstring>
#include <cstdio>
#include <type_traits>
using morrow::blob_upload_proposal_v1::proposal;
using morrow::blob_upload_proposal_v1::bound_input;
static_assert(!std::is_copy_constructible_v<proposal> && !std::is_copy_assignable_v<proposal>);
static_assert(!std::is_copy_constructible_v<bound_input> && !std::is_copy_assignable_v<bound_input>);
static_assert(std::is_nothrow_move_constructible_v<proposal> && std::is_nothrow_move_assignable_v<bound_input>);
int main() {
    mbp_proposal_view v{};v.abi_version=MBP_VERSION;v.struct_size=sizeof(v);v.byte_length=16777216;
    std::memset(v.whole_sha256,9,32);const uint8_t label[]="C++ example";
    v.label=label;v.label_length=sizeof(label)-1;std::array<uint8_t,32768> data{};data.fill(42);
    v.producer_input=data.data();v.producer_input_length=static_cast<uint32_t>(data.size());
    proposal original;assert(original.assign(v)==MBP_OK);data.fill(0);
    proposal moved(std::move(original));assert(original.native()==nullptr);
    std::vector<uint8_t> wire;assert(moved.encode(wire)==MBP_OK);auto exact=wire;
    assert(original.encode(wire)==MBP_BUFFER && wire==exact);
    v.reserved2=1;assert(moved.assign(v)==MBP_INVALID);assert(moved.encode(wire)==MBP_OK && wire==exact);
    auto broken=exact;broken[16]^=1;assert(moved.decode(broken.data(),static_cast<uint32_t>(broken.size()))!=MBP_OK);
    assert(moved.encode(wire)==MBP_OK && wire==exact);
    mbp_identity id{};id.abi_version=MBP_VERSION;id.struct_size=sizeof(id);
    std::memset(id.transfer_epoch,1,32);std::memset(id.object_ref,2,32);std::memset(id.operation_id,3,32);
    bound_input bound;assert(bound.bind(moved,id)==MBP_OK);moved=proposal{};
    assert(bound.view(v)==MBP_OK && v.producer_input_length==32768 && v.producer_input[0]==42);
    bound_input transferred(std::move(bound));assert(bound.native()==nullptr);
    assert(transferred.encode(wire)==MBP_OK);exact=wire;
    bound_input decoded;assert(decoded.decode(wire.data(),static_cast<uint32_t>(wire.size()))==MBP_OK);
    mbp_identity actual{};assert(decoded.identity(actual)==MBP_OK && std::memcmp(actual.operation_id,id.operation_id,32)==0);
    bound_input assigned;assigned=std::move(decoded);assert(decoded.native()==nullptr);
    // Self move must preserve the original owner and its borrowed data.
    auto *same=&assigned;assigned=std::move(*same);
    assert(assigned.encode(wire)==MBP_OK && wire==exact);
    assert(assigned.decode(nullptr,1)!=MBP_OK);assert(assigned.encode(wire)==MBP_OK && wire==exact);
    std::puts("{\"status\":\"PASS_SCOPED_NATIVE_CPP\",\"store_proof\":false}");return 0;
}
