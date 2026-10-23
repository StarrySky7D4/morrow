# Case023 Windows original47 and nine required groups

Result: **PASS_SCOPED_ORIGINAL47_AND_NINE_REQUIRED_GROUPS**. This closes the original Windows invocation requirement for the exact candidate below. P2, P3, Windows SDK26/G04/FREEZE, overall source acceptance and release eligibility remain open.

- Source SHA256: `93144acd7caea95e50b9547ede5bd7a8cf76022de0455ea1f7715e58e56d6da4`.
- Base commit: `f8ef33ec091a82d68f3c2745dde950f8762d8878` (working changes included in the source digest).
- VM: `ff88df68-1ac5-4e8e-a696-b6fc09d34a84`; case: `a8ab0c6828044578a47d8cc3c8b06ae8`.
- Original47 steps,92 delivered frames,3523 input bytes,107 contiguous journal records;283 independent review assertions.
- Journal SHA256: `82e89968b65da20e44c709e96415fabf9721be3191036c3582b867f7f388380a`.
- Review: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\case023-nine-review\result.json`.
- Evidence source: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019`; original case receipts and fixed public snapshots are retained unchanged.

| Original review group | Actual evidence and scope |
|---|---|
| 1. startup/provenance/security/handshake | journal[88,92] + original source-enforced A1 construction and sealed v8 completion. Original A1 login, security readback and startup enforcement are bound by the exact executed source and success-only opaque completion path. Raw individual startup API values are not separately claimed as serialized observations. |
| 2. sealed-basic files | journal[24,90,92]. Fresh sentinels, actual workspace write and outside PermissionDenied, terminal output hashes and EOF. |
| 3. sealed-basic identity | journal[90,104]. Actual business child primary token: exact owned offline SID, restricted and not elevated; this is evidence, not an authority input. |
| 4. sealed-basic network | journal[24,90]. Owned loopback positive controls before and after plus explicit WSA10013 denial. Complete network isolation remains unqualified. |
| 5. original native/IO/worker closure | journal[92,97]. Same original runner PID/creation/raw DWORD, actual worker joins/physical EOF and consume-only NativeHostCompletion. Legacy serialize-only NOT_PROVEN projection remains unchanged. |
| 6. descendant/input/keeper | journal[92,97] + source-bound native completion. Sealed completion requires original Job/input/forwarder retirement; original invocation-owned desktop closes in take_native_host_completion. No external descendant adoption is inferred. |
| 7. quota/registry/authenticated closeACK | journal[92,97]. Authenticated original terminal close ACK, timely actual SDK monitor join, original slot and durable facts retirement. Scope is this invocation; no global registry reset or >16 refusal qualification. |
| 8. owner/factory | journal[92,94,97] + source-bound factory/owner receipt consumption. Actual task ACK, synchronous original runtime shutdown, captured scheduler native waits/closes, same protected owner finish and original factory identity. |
| 9. owned setup/P/ROOT outer closure | journal[100,103,104,106], original_helper, terminal.case_original.capture, policy_original, closed_sessions. Owned setup and WFP retirement, C source-enforced origin closure; exact P source requires original C/thread DWORD0, Job active0/terminated0 and checked aliases/PI/Job close before exit0. ROOT independently proves P/controller native exit and EOF/joins/disposal/transports. Managed joins do not claim native TLS destructor evidence. |

The restricted business child returned0 and EOF; it wrote the workspace sentinel, received outside PermissionDenied, and observed WSA10013 against an owned loopback listener with positive controls before and after. Its primary token had the exact owned offline SID, restrictions and no elevation. This is bounded basic network evidence, not complete network-isolation qualification.

The same original runner was PID3384, creation FILETIME134372317560831071, raw DWORD0. The same invocation consumed its native completion, authenticated ACK, monitor/slot retirement, original factory and protected-owner receipts. Actual cleanup retired the two owned users, one group, hidden-user entries, four firewall rules, three sandbox directories and16 owned WFP filters plus their unshared provider/sublayer.

The ROOT original P/helper and its primary/input/output-reader threads returned DWORD0, with physical EOF and all36 original slots retired. ROOT separately observed controller5312 exit0, both EOFs, managed reader joins, checked native closes and disposal. Both original transport sessions closed. The policy owner restored its sole newly added right and original holder set. No issued-token revocation is inferred from policy removal.

The old serialized native NOT_PROVEN projection was not promoted to authority; the independent opaque completion chain supplied the required evidence. Individual startup readback values and inner P Job values are source-enforced on the exact pinned success path, not claimed as separately serialized raw observations. Directory entry counts/mtimes changed during setup; original directory identities remained fixed and all executable pins were identical. Managed thread joins do not establish native TLS-destructor proof.

VM025 was saved after case closure. Four VMs remain registered. Historical Unknown owners were not replayed, queried, adopted or terminated. This report grants no cleanup authority and does not qualify later source changes.
