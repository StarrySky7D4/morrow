# Watch observation Wasm qualification candidate

Three separately compiled Rust/C/C++ payload consumers, not a watch host or
production package. They import only the two existing raw task functions and
return the exact accepted Cap'n Proto observation bytes. No filesystem, WASI,
clock, network, account, process, watch, rename or approval import is supplied.

The Rust codec and C ABI inputs remain the original reviewed files. C and C++
share one Rust archive and the existing SDK allocator, never two runtimes.
The additional C++17 status header supports exception-disabled guest builds;
allocation failure is not promised recoverable. The separate native exception
wrapper keeps its previous Windows evidence and is not used in these guests.

The host uses the existing default plugin Runner, not a substitute interpreter.
The scope is actual bounded Wasm codec/memory/state execution on Windows.
Production approval, protected selection, original shared clock/resource budget,
native producer cancellation/close/join, package installation and formal UI must
be qualified separately. SDK26/G04/SDK_FREEZE and release remain OPEN.
