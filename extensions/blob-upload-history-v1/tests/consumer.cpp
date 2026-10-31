#include "morrow_blob_upload_history_v1.hpp"
#include <fstream>
#include <iterator>
#include <cstring>
#include <iostream>
#include <type_traits>
#define REQUIRE(x) do { if (!(x)) return __LINE__; } while (0)
int main(int argc, char **argv) {
  using morrow::upload_history_v1::Packet;
  static_assert(!std::is_copy_constructible_v<Packet>);
  static_assert(std::is_nothrow_move_constructible_v<Packet>);
  REQUIRE(argc == 2);
  std::ifstream file(argv[1], std::ios::binary);
  REQUIRE(file.is_open());
  std::vector<uint8_t> bytes((std::istreambuf_iterator<char>(file)), {});
  REQUIRE(!file.bad() && !bytes.empty() && bytes.size() <= MBH_V1_MAX_PACKET_BYTES);
  std::array<uint8_t, 32> submission{}, nonce{};submission.fill(1);nonce.fill(9);
  Packet owner;REQUIRE(owner.decode(bytes, submission, nonce) == 0);
  MbhView before{}, after{};REQUIRE(owner.view(before) == 0);
  nonce[0] = 8;REQUIRE(owner.decode(bytes, submission, nonce) == 4);
  REQUIRE(owner.view(after) == 0 && after.operation_id.data == before.operation_id.data);
  Packet moved(std::move(owner));REQUIRE(owner.view(after) == 5);
  REQUIRE(moved.view(after) == 0 && after.original_unknown == 1 && after.current_stage == 1);
  REQUIRE(after.revision == 3 && after.outcome == 4 && after.error_code == 7);
  REQUIRE(after.exit_code == INT32_MIN && after.registry_revision == UINT64_MAX);
  REQUIRE(after.operation_id.bytes == std::strlen("upload-原记录"));
  REQUIRE(std::memcmp(after.operation_id.data, "upload-原记录", after.operation_id.bytes) == 0);
  std::array<uint8_t, MBH_V1_TASK_ID_BYTES> task_id{};uint32_t written = 0;nonce.fill(9);
  REQUIRE(mbh_task_id_write(submission.data(), nonce.data(), task_id.data(), static_cast<uint32_t>(task_id.size()), &written) == 0);
  REQUIRE(written == task_id.size());
  std::string_view task_name(reinterpret_cast<const char*>(task_id.data()), task_id.size());
  REQUIRE(moved.decode_task(bytes, task_name) == 0 && moved.view(before) == 0);
  task_id[11] = 'f';REQUIRE(moved.decode_task(bytes, task_name) == 4);
  REQUIRE(moved.view(after) == 0 && before.operation_id.data == after.operation_id.data);
  std::cout << "CPP_HISTORY_CONSUMER_PASS\n";return 0;
}
