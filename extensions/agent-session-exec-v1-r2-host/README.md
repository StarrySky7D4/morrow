# R2 reviewed host adapters

This independent experimental host crate consumes `agent-session-exec-v1`
version 1 revision 2. It supplies a distinct `MROWARE2` Protobuf/LZ4 package
container, a strictly prepared Wasm import, and a dedicated bounded native
frame route. Existing native v2/v3 handshakes and package formats remain separate.

`AgentPackage` includes the original immutable Core Wasm `Package`, the R2 raw
schema identity, a finite sorted session declaration, an execution domain and
capability ceiling. Loading or inspecting it grants nothing. The trusted host
must review the complete archive SHA before `PreparedAgentPackage::approve`,
select an explicit subset and finite expiry, and obtain an opaque original
`ApprovedSession`. All calls use that original Core runtime and connection.
No package declaration can approve itself.

The only additional Wasm import is `morrow_agent_session_exec_v1.call`. Task
read/complete imports retain the original fuel, memory, call count, cancellation
and bounded suspension behavior. Mixed extra imports and legacy factories reject
this profile. The driver routes no Core content exchange. `native::exchange`
checks canonical requests and correlated replies; `exchange_frame` adds one
strict little-endian length prefix with a 128 KiB payload bound.

On Linux, `LinuxFixedExecutor::register` opens a reviewed regular ELF artifact,
verifies its SHA and copies it into a write/grow/shrink sealed memfd. Execution
uses that immutable descriptor, the already opened cwd, an explicitly cleared
environment, fixed stdin, bounded stdout/stderr and a process deadline. Signal
termination is represented as a negative signal number. Excess output or uncertain
input/cleanup returns an error; the SDK preserves Unknown and does not replay.
Native cleanup has a process-wide eight-job bound and stays outside SDK authority
locks. A callback still must be invoked through `execute_claimed` after trusted
review, approval and the winning Claim.

The ELF hash covers the main artifact, not its dynamic loader/shared libraries.
The adapter controls the leader and its POSIX process group, and observes stdout
and stderr EOF; it does not establish containment of arbitrary descendants.
Synchronous OS spawning is not preemptible. This is a fixed-input host adapter,
not production OS sandbox, Windows, authenticated plugin transport or GUI approval
qualification. PTY, interactive input, output streaming and public process-control
interfaces remain deferred.

The real Codex client adapter uses the client-only SDK in a separate process to
avoid linking two incompatible SQLite libraries. Its native fixture and caller
qualification are recorded separately from this package/import route.
