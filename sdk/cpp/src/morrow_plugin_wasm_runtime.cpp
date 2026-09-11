// libc++ explicitly permits replacing its verbose-abort hook. A guest error
// traps locally; it must not pull stderr, clocks or other WASI host imports.
#include <__verbose_abort>
#if !defined(__wasm32__)
#error "This adapter requires wasm32"
#endif
[[noreturn]] void std::__libcpp_verbose_abort(const char *, ...) noexcept {
  __builtin_trap();
}

// The no-exceptions libc++abi fatal path must also remain inside the guest.
extern "C" [[noreturn]] void __abort_message(const char *, ...) {
  __builtin_trap();
}

// Every invocation owns a fresh instance. Initialize C++ globals explicitly;
// let the host reclaim instance memory instead of WASI command-exit cleanup.
// Automatic/local destructors still run on normal return from mp_guest_run.
#include <stdint.h>
extern "C" void __wasm_call_ctors();
extern "C" int32_t mp_guest_run();
extern "C" int32_t morrow_run() {
  __wasm_call_ctors();
  return mp_guest_run();
}
