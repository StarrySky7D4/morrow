(module
  (import "morrow_probe_v0" "exchange"
    (func $exchange (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $entries (export "entries") (mut i32) (i32.const 0))
  (data (i32.const 0) "first")
  (data (i32.const 16) "second")
  (func (export "run") (result i32)
    (local $seed i32) (local $first i32)
    global.get $entries i32.const 1 i32.add global.set $entries
    global.get $entries i32.const 1 i32.ne if unreachable end
    i32.const 41 local.set $seed
    i32.const 64 i32.const 99 i32.store
    (call $exchange (i32.const 0) (i32.const 5) (i32.const 128) (i32.const 32))
    i32.const 4 i32.ne if unreachable end
    i32.const 128 i32.load local.set $first
    (call $exchange (i32.const 16) (i32.const 6) (i32.const 132) (i32.const 32))
    i32.const 4 i32.ne if unreachable end
    i32.const 64 i32.load i32.const 99 i32.ne if unreachable end
    global.get $entries i32.const 1 i32.ne if unreachable end
    local.get $seed local.get $first i32.add
    i32.const 132 i32.load i32.add))
