#include "morrow_blob_upload_history_task_v1.hpp"
#include <fstream>
#include <iterator>
#include <iostream>
#include <type_traits>
#define REQUIRE(x) do { if (!(x)) return __LINE__; } while (0)
int main(int argc, char **argv) {
  using morrow::upload_history_v1::TaskPacket;
  static_assert(!std::is_copy_constructible_v<TaskPacket>);
  static_assert(std::is_nothrow_move_constructible_v<TaskPacket>);
  REQUIRE(argc == 3);
  std::ifstream good(argv[1], std::ios::binary), bad(argv[2], std::ios::binary);
  REQUIRE(good.is_open() && bad.is_open());
  std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(good)), {});
  std::vector<uint8_t> wrong((std::istreambuf_iterator<char>(bad)), {});
  REQUIRE(!good.bad() && !bad.bad());
  TaskPacket owner;MbhView before{}, after{};
  REQUIRE(owner.decode(bytes, "history.inspect", "morrow.blob.upload.history.v1") == 0);
  REQUIRE(owner.view(before) == 0 && before.original_unknown == 1 && before.current_stage == 1);
  REQUIRE(owner.decode(wrong, "history.inspect", "morrow.blob.upload.history.v1") == 4);
  REQUIRE(owner.view(after) == 0 && before.operation_id.data == after.operation_id.data);
  REQUIRE(owner.decode(bytes, "other.handler", "morrow.blob.upload.history.v1") == 2);
  REQUIRE(owner.decode(bytes, "", "morrow.blob.upload.history.v1") == 1);
  TaskPacket moved(std::move(owner));REQUIRE(owner.view(after) == 5);
  REQUIRE(moved.view(after) == 0 && before.operation_id.data == after.operation_id.data);
  TaskPacket assigned;assigned = std::move(moved);REQUIRE(moved.view(after) == 5);
  REQUIRE(assigned.view(after) == 0 && before.operation_id.data == after.operation_id.data);
  std::cout << "CPP_HISTORY_TASK_CONSUMER_PASS\n";return 0;
}
