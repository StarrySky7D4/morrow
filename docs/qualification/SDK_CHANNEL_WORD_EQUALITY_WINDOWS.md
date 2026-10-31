# Windows SDK channel word comparison qualification

The Rust SDK compares every payload byte when a retained channel `Frame` is checked
against an independently owned snapshot. On wasm32, the previous byte-wise fallback
spent about 295,162 instruction-fuel units comparing equal 32 KiB responses. A safe
eight-byte comparison with an exact remainder check measured 37,124 units for the
same diagnostic. Native targets retain ordinary slice equality. Identity, cursor,
length, canonical wire encoding and SHA-256 checks remain intact.

The isolated candidate completed both original five-by-32-KiB byte/event cases:
16 metered calls, all five original ACKs, explicit Close, exact payload digest and
actual original producer join. It used 15,274,745 and 15,286,851 of the unchanged
20,000,000 fuel units. Memory remains 16 MiB and the call limit remains 16.

The four-by-64-KiB case still exhausts fuel. Its optimized executable reaches
11 calls and ACK3. The new `channel_word_equality_sdk` profile checks that exact
position, no business output, original Control revocation, durable ACK prefixes
and actual producer join for both bytes and events. The old
`channel_bounded_sdk` profile retains its historical 8-call/ACK2 boundary and red
five-frame results. Old fixture `ARTIFACT_SHA256SUMS`, source locks and `sdk/compat`
originals are not rewritten to describe new executables.

Run the current profile from Windows with the installed Rust/capnp toolchain:

```powershell
python -B tool/windows/verify_sdk_channel_words.py --run-id 1242 --evidence-root D:\build_cache\_audit\windows-restriction-lineage-20261022-019 --refresh-copied-sdk-locks
```

Choose a fresh numeric run ID and an existing evidence directory below
`D:\build_cache`. The driver copies the four original fixture sources, explicitly
refreshes only their copied SDK source locks, compiles fresh Wasm and host artifacts,
records their independent hashes and executes all eight current-profile assertions.
Historical source locks and artifact pins remain unchanged. This is a bounded
Windows Wasmi/SDK result, not Windows native process-chain P2, full SDK26, SDK freeze
or release approval. Those gates remain open. The initial diagnostic's missing
schema compilation failure is retained separately; the corrected complete input
copy passed all 93 native Rust SDK tests and 1,656 actual Wasm mutation checks per
variant. Diagnostic and runtime counts are distinct and are not a release total.

Current canonical source `1a107628876dc3e437a44c5b2aafb5ea5e47fb27e11bd6d0603b2ce793d50a34` completed run1242: eight Windows Wasmi methods passed, with original processes closed. Its five-frame ByteStream/Events measurements are 15,273,429 / 15,285,535 fuel; these are separate from the isolated candidate measurements above. Run1245 passed 93 native Rust SDK tests against the current core schemas. The 1,656 mutation checks per variant came from the earlier actual Wasm diagnostic1234; they were not additional native test methods. [Current evidence](evidence/resume-20261030-current/sdk1242-current-windows-matrix.json).

Diagnostic1234 paired 295,162 with 37,124 fuel for equal 32 KiB responses; diagnostic1225 separately measured 295,159. Original logs remain unchanged. Native run1282 and review1283 bind source `797e3e2d10c089c79a46af94972c85b54906b32adf84fc3911b98bf54a2737ec`: 93 Rust tests, 14 C/C++ programs and three independent SDK consumers passed with current core schemas. This does not qualify protected host or UI execution. [Native matrix evidence](evidence/resume-20261030-current/sdk1283-current-native-review.json).
