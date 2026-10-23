# Windows Cap'n Proto transport and cases028/029

The derived parent/Runner transport and v8 native-close/authentication bodies now use typed Cap'n Proto schemas. Forty-three selected, actually executed codec/state-machine tests and case028's original47/nine groups passed on source `fe7de7d538bb93bc85c7148e61fea0e04b805e7da97f1454cc1f3b1b01c8bae5`. Case029's caller-refusal continuation did not pass the outer parent exit gate. Full P2/P3, Windows SDK26/G04/FREEZE, source acceptance and release eligibility remain open.

The schemas are in the derived `windows-sandbox-rs/schemas` directory. Runner variants include bounded raw byte streams and the complete spawn permission profile. V8 messages and authentication material use typed finite facts, not JSON embedded in Data. Receivers enforce framing, size, depth/traversal limits, known unions and canonical bytes. JSON, truncated frames, unknown fields and noncanonical encodings are rejected. Schema digests are generated from source and used by the SDK and runner. The separate legacy machine-wide provisioning service still has its existing JSON contract; these results do not claim that the entire project has no JSON.

## Frozen executable evidence

All four roles were compiled from mechanically bound immutable source snapshots. S's first compiler invocation failed with disk-full error112; that failed attempt remains retained. After verified cache reclamation and lossless compression, a separate compiler-only S invocation completed on unchanged sources. No failed native case was replayed.

| Role | SHA256 |
|---|---|
| P | `b2218a64f75dbab13f802f89b42d0e01fe0828175a9309b32c6ab1bac6f14978` |
| C | `dcbce7b8f66f62e5ae549ae719781da04801cfc68d37cb8fc299865e6f0c860d` |
| R | `8f25f55545c0ac7a1c2fb5e482c4878a17acae281940e8885ae2c1d6bef91cc8` |
| S | `768ecd4765bde1e76ed361f1fc47896f61b34d85b8850673f1baaec9f1e260d4` |

Case028 `ba42e3aef39845e7b856c5f1a35e58c7` returned all 47 original steps, 92 input frames/3523 bytes and 107 journal records. The independent review passed 284 checks in all nine required groups. Native close/ACK, EOF, original joins, SDK slot retirement, owned setup/account/WFP cleanup, original P exit and same-owner conditional policy restoration closed. Both original transports closed. Journal SHA256: `33a61bd4cbc49e70cb2e219ed4cddeffe5c5847d192c02b3345976168b33a44f`. The negotiated Runner schema was `e798cb3715c40d9e6576f30262fb164413384782b513f97ee36e80ead5fc51c7`.

Case029 `260d46acdd434677bcbf6ed17bffc265` used the same four binaries in a new caller-refusal case. Its inner P2 sequence returned 47 steps and wrote 107 records. The inner journal reports original refusal delivery, local SDK slot retirement and owned setup cleanup, but those records do not prove outer P/ROOT completion. A separately closed, read-only transport copied existing public journal/controller files; another copied the two exact existing P log files. Neither transport contacted the original lifecycle owner.

The 537-byte P stderr frame has a valid CRC and matches the controller's captured prefix. Its source decoder reports `IoErrorLatched`, `ObserveChild`, completed `WaitChild`, `InvalidData` with no raw OS error. It does not identify the rejecting predicate, native exit DWORDs or Job accounting values. `actual_failed_predicate=NOT_OBSERVED`, `policy_restore=NOT_OBSERVED`, `case_clean=false`. The old invocation is retained and is not polled, replayed, adopted or terminated.

New source adds finite predicate diagnostics and distinguishes retained-owner observation, primary-thread exit query, Job accounting query and final proof validation. The acceptance predicates, one-shot observation budget and no-replay policy remain unchanged. These later edits are not covered by case028's frozen source evidence.

Evidence root: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019`: `ipc-capnp-001/qualification-002/result.json`, `roles-023/result.json`, `case028-nine-review/result.json`, `case028-summary.json`, `case029-unknown-review/observed.json` and its original stored snapshots. Host inventory later observed VM027 Off; this is not evidence of natural P exit or policy restoration. Do not resume it as a clean environment.


## Later exact-source cases030 and031

Source `51fba03227c484d6a2757b367522e4be6e32e3f2d9234f02644963a56caaf619`
adds finite parent exit diagnostics without changing any acceptance predicate,
native polling budget or retry rule. Twenty-eight actual selected pure tests
passed. Roles024 were rebuilt and byte-bound from this source.

Case030 `6ae3a36a1ebd4447ae19303df2309b7a` passed 355 checks for one actual
caller refusal, including original SDK/factory retirement, setup cleanup,
outer parent exit and same-owner policy restoration. Case031
`981952f307b44bceb88be9a983ac6937` passed the normal original47/nine groups
with 284 checks. Its 107-record journal SHA256 is
`263f76287a3ed6763c5d188212814947fad4d7ae0cfa5aefdfef8b831af42097`.
Both cases' original transports closed. VM028 was saved after completion;
four VMs remain registered.

The original case029 local retained receipt later recorded policy transport
Unknown after restore was reserved. Policy restoration is not proven; neither
that case nor its original session was polled or replayed. VM027 was observed
Off, its configuration and disk were archived and verified, and only its VM
registration was removed. Its disk remains retained. New VM028 uses a fresh
clean parent disk. Its runtime state is under `C:\MorrowQA`; all build caches,
compiler snapshots, logs and audit evidence remain under `D:\build_cache`.

Evidence: `parent-exit-qualification-002/result.json`, `roles-024/result.json`,
`case030-refusal-review/result.json`, `case031-nine-review/result.json`,
`case029-summary.json`, `vm-archive-016/result.json`, and
`vm028-case031-saved-state/result.json` under the audit root above.
These scoped passes do not complete the full P2/P3 or SDK gates. Later UI
recovery changes are a new source candidate and require their own verification.
