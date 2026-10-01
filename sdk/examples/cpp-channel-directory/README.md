# Channel Directory consumer (cpp)

Handler: `channel.directory.consume`. Input type: `morrow.channel.directory.v1`; the input is exact canonical Directory bytes. Output type: `bytes`; this example explicitly returns a 64-byte CHV1 digest/status summary.

The example requires exactly one endpoint. Its kind selects byte-stream mode 0 or events mode 2; there is no 65-byte test adapter. It consumes at most 32 frames subject to endpoint message/request budgets, ACKs exact frame sequence/SHA/cursor, and never replays uncertain operations.

Use a fresh candidate/project identity to compile and package this source. See [CHANNEL_API.md](../../CHANNEL_API.md) for caller prepare/append/run-once/status/close, buffer ownership and outcome semantics. Source inclusion does not claim a completed native/Wasm qualification.
