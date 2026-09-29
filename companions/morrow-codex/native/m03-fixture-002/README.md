# M03 native fixture 001 (version 0.4.0)

Separate controlled fixture candidate, copied from frozen native003 and linked only to the new qualification/m03-fixture-001 adapter. Frozen upstream Core, qualification002, native003, wire-kit001 and pipe-kit001 are not edited. No new v3 commands or ordinary plugin authority are introduced.

Native CLI requires the usual fixture-base/evidence-dir/max-chunk pairs and one final --fixture-spec-sha256 DIGEST pair; max-chunk is fixed to 1024. The eight host client arguments bind the exact spec bytes in the existing evidence directory. Core mode uses the actual upstream events and the unchanged two cancellation gates. Data-pending mode pauses the single read owner immediately after DataBound is completely decoded and its read reaped, before another read can be issued. No CancelAll-based pause/resume exists. Request writes, control handling, deadlines and cancellation cleanup continue.

All native003 sticky control failures, Close facts and terminal observation are retained. Reader/router/writer handles are additionally retained, polled for completion and joined only in the SAME existing 500ms Close budget, followed by the evidence writer in that budget. Router end is not thread join. Missing completion/failed evidence remains unconfirmed. Aggregate Unknown and failed Core outcomes never become success; a matching Close ACK cannot clear prior failures.

Local tests cover synthetic state/codec/async barriers, read-boundary guards, local evidence files and owned local threads. They do not run this executable in native mode, launch a host, use an OS data pipe or issue HTTP. No runtime qualification is claimed. Exact schema and remaining runtime obligations: receipts/m03-fixture-001/fixture-interface.md.
