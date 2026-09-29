# Fixed host candidate001: targeted static seam review

One confirmed protocol pairing blocker; no runtime reproduction, host/client invocation, build or test run. Source file hashes were rechecked against the fixed manifest. Intake receipt is ../native-host-intake-001/receipt.json (23 referenced files matched).

Candidate manifest SHA-256: 2d02eb5a7afdae7b4ebc76b872536723e1b4b5ffa1f4b7ee624cffef0417c587.
Executable SHA-256: 5ba87dd8798fb5ce09c3bf7ed5f841aa97410e0d618d70f6c2579453b72e2c49.

## H1: host never acknowledges guest Close using the reply expected by the plugin

Fixed source `source/src/supervisor.rs:891` handles a valid Close by setting reason=25 and continuing. At lines 787-808 the next iteration starts closing, clears pending control output, and queues sequence-zero Stop. Lines 871-872 close the control pipe after Stop is written. This path never generates a sequence-echoing State/code0 reply for Close.

The fixed plugin001 `native/m03-stream-001/src/session.rs:270-275` only sets close_acked for State/code0 corresponding to the pending Close. Its wait_close at lines 503-508 requires that acknowledgement. `src/main.rs:160-162` gives it 500 ms and otherwise fails. The plugin's announced002 design also retains Close acknowledgement waiting; that announcement is not yet a frozen002 source verification.

Consequently even an otherwise normal HTTP/Core completion cannot satisfy the plugin's clean Close contract when paired with this host. This is a static mismatch, not an executed failed pairing.

Correction boundary: keep host001 frozen and publish a new candidate which enqueues and fully writes the matching Close reply before shutting its control pipe or otherwise communicates an explicitly agreed successful terminal response. Do not clear or overwrite an in-flight frame. Preserve failure on partial write, original deadline or bounded close timeout. Stop/revocation must remain distinct from successful Close acknowledgement; whole owner release still requires externally observed child exit, both stdio EOFs and resource cleanup.

Targeted verification: a normal RequestClosed -> guest Close -> matching State/code0 fully written -> guest clean exit sequence; verify any interrupted/partial Close reply remains failed/unconfirmed. Do not rerun old transport/codec/platform suites.

## Remaining requested boundaries: source observations only

The send ticket keeps network_pending owned through installation and completion of the opening task (pipe_driver.rs:24-39; supervisor.rs:326-342). Native grant consumption and durable Core Unknown precede network_ticket; failures do not restore resend authority (http_authority.rs claim). Original deadline reaches SendContext and the live gate.

HTTP EOF invokes actual response-material storage and Core observation before setting Observed; cleanup does not rewrite it to Unknown (http_authority.rs:observe; supervisor.rs:585-595). RequestClosed waits on joined pipe/HTTP resources and no pending ticket; owner Released additionally waits on child exit and both stdio EOFs (supervisor.rs:680-688,810-835), then authority persists the owner snapshot before dropping its pin (authority.rs:741-779).

Old-generation admission identity is checked before the revoked cleanup whitelist; fresh work is denied, while Query/Cancel/Close and zero-window classified issued-tail credit remain eligible (supervisor.rs:702-706,883-892). Absolute credit counters are monotone and bounded by issued bytes; quota equality requires a terminal probe; normal pipe cancellation waits for final peer ACK and completed data writes (supervisor.rs:345-361,621-678).

The 32-byte v3 endpoint reference is mapped to 64 lowercase hex bytes for the existing Core reference contract (http_authority.rs:265-268); the wire reference remains unchanged. These observations do not grant native-runtime or full matrix credit. The producer's five API tests use simulated owner records plus actual Store/ledger and were not independently rerun.
