# Owning C/C++ observation interfaces

This experimental crate depends only on the adjacent `fs-watch-v1` codec.
`include/morrow_fs_watch_v1.h` defines 13 C functions and the pointer/ownership
contract. Frames own copied bytes; change views borrow immutable raw UTF-16.
Invalid outputs remain unchanged; pointer arithmetic is not memory isolation.
Allocation failure is not promised recoverable.

`morrow_fs_watch_v1.hpp` is a native exception-based C++17 wrapper.
`morrow_fs_watch_v1_status.hpp` is the move-only status-returning wrapper used by
exception-disabled Wasm builds. Both own local bookkeeping only. State disposal
never cancels, closes, joins, approves, resets or refunds a host watch.

`tests/consumer.c` and `tests/consumer.cpp` take six exact native payload paths
and a separate synthetic vector directory. `synthetic_vectors` emits seven
explicit protocol fixtures; none is an OS observation. The sibling
`fs-watch-v1/qualification` workspace exercises actual Rust/C/C++ Wasm using the
existing default Runner. Original pre-integration evidence is retained under
reviews1445/1455; current repository builds still require new evidence.

No production host route is enabled. SDK26/G04/SDK_FREEZE remain OPEN.
