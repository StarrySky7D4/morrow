// Exercise libc++ allocations while the Rust codec allocates and frees replies.
#define morrow_run rename_and_decode
#include "../examples/cpp-rename/plugin.cpp"
#undef morrow_run
#include <cstdlib>
#include <cstring>
extern "C" int32_t morrow_run() {
  struct alignas(256) Aligned {
    unsigned char bytes[512];
  };
  std::vector<Aligned> aligned(8);
  if (reinterpret_cast<uintptr_t>(aligned.data()) % 256 != 0)
    return 99;
  aligned[3].bytes[17] = 81;
  std::vector<std::string> keep;
  for (int i = 0; i < 32; ++i)
    keep.emplace_back(2048 + i, static_cast<char>('a' + i % 26));
  auto *p = static_cast<unsigned char *>(std::calloc(31, 7));
  if (!p)
    return 99;
  for (int i = 0; i < 217; ++i)
    if (p[i] != 0)
      return 99;
  p[0] = 42;
  auto *q = static_cast<unsigned char *>(std::realloc(p, 4096));
  if (!q)
    return 99;
  if (q[0] != 42)
    return 99;
  auto result = rename_and_decode();
  for (int i = 0; i < 32; ++i)
    if (keep[i] != std::string(2048 + i, static_cast<char>('a' + i % 26)))
      return 99;
  if (aligned[3].bytes[17] != 81)
    return 99;
  std::free(q);
  return result;
}
