/* Build only into freestanding Wasm guests. All languages share Rust's
 * allocator. Unavailable process/diagnostic operations trap locally, never call
 * WASI. */
#if !defined(__wasm32__)
#error "This adapter requires wasm32"
#endif
#include <stdint.h>
#include <stdlib.h>
extern void *mp_guest_malloc(uint32_t);
extern void *mp_guest_calloc(uint32_t, uint32_t);
extern void *mp_guest_realloc(void *, uint32_t);
extern void mp_guest_free(void *);
_Static_assert(sizeof(size_t) == 4, "wasm32 only");
void *malloc(size_t size) { return mp_guest_malloc((uint32_t)size); }
void *calloc(size_t n, size_t size) {
  return mp_guest_calloc((uint32_t)n, (uint32_t)size);
}
void *realloc(void *p, size_t size) {
  return mp_guest_realloc(p, (uint32_t)size);
}
void free(void *p) { mp_guest_free(p); }
_Noreturn void abort(void) { __builtin_trap(); }

extern void *mp_guest_memalign(uint32_t, uint32_t);
void *aligned_alloc(size_t alignment, size_t size) {
  if (!alignment || (alignment & (alignment - 1)) || size % alignment)
    return 0;
  return mp_guest_memalign((uint32_t)alignment, (uint32_t)size);
}
int posix_memalign(void **result, size_t alignment, size_t size) {
  if (!result || alignment < sizeof(void *) || (alignment & (alignment - 1)))
    return 22;
  void *p = mp_guest_memalign((uint32_t)alignment, (uint32_t)size);
  if (!p)
    return 12;
  *result = p;
  return 0;
}
