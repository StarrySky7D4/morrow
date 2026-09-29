# M03 native v3 / real Core fixture client, revision002

This new crate consumes the revision002 `qualification/m03-stream-002` Core entry,
host v3 wire-kit-001 and shared Windows pipe-kit-001. It defines no new schema,
Windows FFI, HTTP client, credential lookup, reconnect or automatic resend.

The executable's native path calls `request_task::start`, which invokes the real
frozen Core ModelClient → Responses API → upstream SSE parser. Stdout is binary
v3 control only. Actual Core events and task outcomes go to new files in a
trusted-harness-created batch directory under this plugin's `out` tree.

```
morrow-codex-native-http-client.exe --morrow-native-http-v3 --fixture-base http://127.0.0.1:PORT/v1 --evidence-dir ABSOLUTE_BATCH_DIRECTORY --max-chunk 1024
```

All arguments form part of the host's exact LaunchSpec/config binding. The guest
checks its actual executable hash/PID against Challenge and precisely echoes the
opaque host configuration hash. It cannot independently reconstruct the host's
approval context from argv; the host and joint harness verify that context.
No CLI argument, native capability bit or proposal matching approves HTTP.

Welcome and exact DataOffer/DataBind/DataBound precede Prepare. Prepare's direct
State response must precede RequestChunk upload, even though the two lanes can
arrive independently. Proposals bind actual Core body/header bytes and the lower
requested response cap. Approved values are compared exactly before a single
Commit. Host-owned validation, durable claim and send fence remain authoritative.

One dedicated thread owns Pipe, including CreateFile, every read/write, individual
completion polling, cancellation recovery and Drop. No async/control thread owns
blocking reaping. Cancellation first closes local business delivery and signals
the owner; cleanup separately waits for RequestClosed and an actually finished,
joined data thread. Unexpected OS errors preserve unconfirmed cleanup. OS issue
and completion evidence count physical frame bytes, independently of body offsets.
Started.pending alone is not sustained backpressure evidence.

The session owns a two-chunk queue and the pending data read. Dropping a read wait
does not drop that queue or OS operation. A control HttpTerminal records a final
offset; only a fully received and delivered contiguous prefix can become stream
EOF. Reaching the byte cap is not EOF. ACKs coalesce in roughly8KiB increments,
flush the final consumed prefix, and preserve parser/drain/error/cancel classes.
An unclassified last parser-delivered chunk stays explicitly unACKed on cancel;
the driver never relabels it as cancellation discard just to close a prefix.

After cancellation, original generation values permit only historical Query,
idempotent Cancel, zero-window tail credit and Close, all with normal identity and
sequence checks. New Commit, RequestChunk or positive-window recovery is refused.
Reserved control ordinals keep room for cancellation and Close. No fresh data read
or write is issued after local cancellation wins the state/issue lock.

RequestClosed is request-level cleanup. The guest never waits for its own process
exit or diagnostic EOF. Whole-session release and real child/stdio termination are
external host/operator observations. Blocking stdio is isolated on dedicated
threads; a broken or partial control lane causes an unconfirmed process outcome,
not a fabricated I/O cleanup receipt or guaranteed thread/process-tree cleanup.

`core-events.jsonl` contains real typed-event metadata and text hashes, not raw
frames or nonces. A trusted harness can use the flushed first real delta as its
HTTP fixture EOF gate; the guest never writes server control. `result.json` keeps
Core terminal/end_turn, transport drain, HTTP evidence, request cleanup, local OS
completion events and local data-thread join separate. Exit0 means the controlled
request completed and cleanup was confirmed; Completed(false) remains false and
does not claim a whole model turn or product success.

Revision001 remains frozen. Revision002 adds a synchronous cancellation latch
shared with Core, including pre-binding replay, and uses the same short gate for
event enqueue and final delivery. Host Stop/Denied/revocation and fatal protocol
errors close that gate without waiting for another network read. A previously
delivered event cannot be retracted; queued and reserved events that lose the gate
are suppressed. Existing model completion and durable HTTP facts remain recorded.

Protocol errors stay fatal while cleanup continues. Close ACK is still consumed,
but cannot erase an earlier failure. The result snapshot is written after Close
observation, includes both close_result and final_control_result, and either error
prevents exit0. This does not wait for this process's own exit or owner release.

Current checks are compilation, nine native local state/async-ownership tests and
four Core-entry local lifecycle/policy tests only.
Those tests do not launch this native mode, a host, pipe, HTTP fixture or Core
request. Frozen producer/independent codec and platform results are dependencies,
not this client's runtime credit. Fixed-candidate host integration is still required
for approval, actual first delta, no-retry server counts, data credit/OS pending,
cancel races and external release. Product gates remain unchanged.
