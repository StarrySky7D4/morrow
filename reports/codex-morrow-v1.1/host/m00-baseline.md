# M-00 baseline and M-01 ownership decision

Actual checkout: `C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor`.
Branch: `codex/io-safety-refactor`; HEAD: `88557916aabf2e10619b1022110035178498898a`.
Initial worktree was clean before participants wrote their scoped outputs. The old
root `codex/ArkTsUI` checkout is not an implementation target. No applicable
AGENTS.md was found in the checkout or its ancestors.

`m00-inputs.json` captures 178 existing inputs, raw SHA-256, LF-normalized protocol
hashes where applicable, all plan file hashes and actual preflight results.
Its SHA-256 is `391d252a7a9bbc8ef7d76222ed5a043b4a267b61fe3ba7c4ca4088eefb43d5e7`.
Raw file identity is not interchangeable with the existing SDK's normalized
protocol digest. The captured plan includes source locks Codex
`44fe510ce3ee61c8ef623adcbf89b901c73ddd61` and CC Switch
`846de29c13ac4d65f164db8c15dd5fd58e29f972`; their actual source availability is
owned by the plugin workstream, not proven by this host receipt.

E = existing source/interface; A = adaptation of existing capability; N = new
implementation needed; Q = compile/device/service qualification still required.

| Capability | E | A / N | Q in this phase |
|---|---|---|---|
| Native session | Wasm guest lifecycle and private workbench protocol | N separate native admission, binding, drain/exit | Only fixture contract; OS identity unverified |
| Incremental stream | `network_node/src/client.rs` aggregates bytes before returning; `plugin_runtime/src/io_binding.rs` and `io_execution.rs` provide IO policy | A reuse destination/policy; N bounded stream | No live network stream claim |
| Session events | `core/src/store.rs` transactions/audit | A transaction infrastructure; N writer/append/tail/checkpoint/outbox | In-memory fake only; no crash/disk proof |
| Agent content | Seven exact SDK commands in `sdk/rust/src/protocol.rs` | A restricted bridge; N collection/large content extension | No model external-send approval/runtime proof |
| Tool dispatch | Existing mutation/IO intent and receipt mechanisms | A exact-input principles; N A/B permit/claim/outbox | Single effective fake claim only, no process |
| Stream view | Existing UI1 schema, Flutter renderer/lifecycle | A renderer; N durable subscription/terminal projection | No Flutter build or device UI test |
| Native package/bundle | `core/src/plugin_package.rs` single Manifest/module Wasm package | N native members/install selection; preserve old decoder | No native package or installer implemented |
| Credential use | Existing controlled service/credential infrastructure | A protected references; N account epoch/secret-use bridge | No account, key, MCP or login read/run |

Confirmed preflight: rustc 1.95.0 (LLVM 22.1.2), Cargo 1.95.0, Cap'n Proto 1.4.0,
Python 3.14.5. Installed targets include x86_64-pc-windows-msvc,
wasm32-unknown-unknown and aarch64/x86_64-unknown-linux-ohos. Installed target is
not platform acceptance. `python tool/verify_plugin_sdk_baseline.py` exited 0
(36 pinned files, 13 original Wasm/package pairs, no rebuild/repack).
`python tool/sync_plugin_sdk_contracts.py --check` exited 0. The independent
read-only audit also matched all 17 transport-v1-rc1 pinned entries. No full
core/runtime/workbench build or old guest execution has run in this phase.

Decision: `contracts/experimental/agent_host_v1/agent_host.capnp` is the sole new
host wire authority. Major 1/revision 1 is experimental, exact raw schema digest
required; incompatible/unknown identity fails closed. Keep all legacy schema,
SDK and frozen Wasm/package originals unchanged. New crate is independent and
does not link host internals. Four minimal families are implemented first;
content/view/package+bundle remain documented drafts. No model semantics enter
the host. Net/IO gain no custom DLL.

Intended export: `reports/codex-morrow-v1.1/host/host-kit/manifest.json`.
Consumers must wait for a complete successful manifest and verify every entry.
Initial status: being implemented; this report is M-00, not a ready-kit receipt.
Rust target: `build/agent-host-g0-target` (isolated). New tooling uses fresh
exclusive output directories and writes success manifest last; missing inputs
and stale outputs must fail nonzero. Joint evidence remains under `../joint/`;
dispatch and plugin repository are not written by this host workstream.
