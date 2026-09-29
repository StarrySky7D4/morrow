# Host005: pipe-owner observations

Same v3, platform pipe, native ledger004, Core and limits. The I/O owner records
one fixed Instant domain per unique pipe instance; ns is duration encoding, not
hardware frequency/resolution. Supervisor event at_us remains receipt time.
Bounded records cover each OS write issue, at most three incomplete samples per
actual operation, cancellation receipt and its pre-CancelIoEx poll, cancellation
request, and actual reap. All carry the actual operation ID and issue ordinal,
frame offset/request length and body end. Partial-tail reissue resets sampling.
Samples require poll-before minus previous poll-after >=25ms. No samples are
manufactured during cancellation. Cancellation's extra poll can consume a real
completion; that exact result goes through normal reap handling once. Completed,
no-operation and unconfirmed-error probes are not pending qualification.

Three new local tests pass: real same-process nonreading pipe, 8192-byte write
with original 1024 requested buffers, three spaced incomplete samples, fourth
cancel probe and real error995 reap/join; early cancel remains fewer than three
samples; synthetic completion/error classifier distinction. No child, protocol
guest, Core request or HTTP was launched. The synthetic classifier test is not
a deterministic OS completion-at-cancellation race test. No partial-write fault
qualification or M03/product gate credit. Frozen host004 remains the A baseline.
