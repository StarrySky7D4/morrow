# test.52 runtime IO interaction

Date: 2026-09-15
Branch: `track-a/w1-io-contract` (not merged)

## Done

- `Runner::new_io_task` accepts only the fixed `morrow_io_v1.call` import
- Ordinary task/dependency runners still reject that import
- IO + dependency combination is `UnsupportedAbi`
- `run_task_with_io` shares the host-call counter
- Callback `Err(())` is a negative transport result, not a successful payload
- IO after `complete` or before `read_input` is a protocol fault
- `PreparedPackage` routes `io-v1` modules to the IO runner and refuses `run_task` without an IO callback

## Not done

- Workbench file picker
- Guest C/C++/Rust SDK helpers
- Pool scheduling of IO workers
- HTTP GET
