# Windows native execution integration candidate

This new trusted interface connects the original Core/R2 authority to the actual
upstream `ExecBackend` and `ExecProcess` public interfaces. It does not implement
or replace the frozen companion's `ReviewedExecConnection`, and does not depend
on that companion's SQLx adapter. The original Core owner, Store, connection,
admission, execution fence, and durable invocation ledger remain authoritative.

`NativeR2State` owns those original objects. `with_state` and `with_runtime` borrow
them under its shared mutex; callers must not reenter that mutex from a provider.
`WindowsExecutionPort` accepts the actual backend returned by upstream
`Environment::get_exec_backend`. It requires a continuously driven Tokio runtime
with at least two worker threads. Current-thread runtimes are rejected.

The caller proposes, reviews, approves, and claims through original R2 APIs.
`approve_fixed` produces an opaque native review and holds Windows file handles
for the actual executable and its directory ancestors, plus the working
directory and its ancestors. It rejects reparse points and nonlocal paths and
checks the real file hash. `start_claimed` calls the actual backend only inside
the original `execute_claimed` callback, after durable `invocation_started`.
It never retries an uncertain start. Before delivering the real provider it
uses original `validate_started_tool` to check execution authority again.

The execution domain hashes the entire serialized upstream `ExecParams`, with
sorted object keys; sandbox, network, and environment policy fields are retained.
This version accepts explicit `params.env` with `env_policy=None`, which the
actual local backend uses without inheriting the parent environment. It rejects
keys scrubbed by the original upstream environment rules, as well as shell
restoration and argv0 overrides. Interactive input uses separately authorized
process controls. The original full parameters are passed unchanged to start.

Read pages and completion facts come from the actual process's bounded event
replay. Original sequence numbers, retention floors, gaps, stdout/stderr,
exit, and EOF are kept distinct. A missing prefix, failed stream, or exceeded
fact budget leaves durable completion unknown; no missing output hash is
invented. Successful completion facts reconcile the exact original tool
identity and revision without renewing authorization. The new process host
checks original executor authority before effects and at receipt delivery.

All ports sharing one state share sixteen native execution slots. Pending
starts, delivered providers, late starts, uncertain effects, cleanup work, and
blocking completion commits hold the same slots. A cleanup timeout reports
unknown and retains the actual process and artifact handles. Trusted callers
can inspect `cleanup_status` and invoke bounded `reap_cleanup`; quota is released
only after real exit and EOF with no provider or pending job remaining.
An unstarted reservation may be returned immediately only when the backend has
provably never been invoked; that does not qualify an uncertain start as closed.
Unresolved entries retain the registry even if external owners are dropped.
Trusted hosts must keep the original state/cleanup port and driven runtime
reachable until `cleanup_status` is empty. Dropping every cleanup handle or
stopping that runtime cannot establish real EOF; unresolved resources then stay
retained. This bounded retention is deliberate and is not a claim of globally
leak-free shutdown.

Close-input and PTY resize are explicitly unsupported because these upstream
traits have no corresponding operations. Windows pipe interrupt is exposed
only for the actual supported native path; PTY interrupt is unsupported.
Mutation acceptance means a backend acknowledgment, not observed process exit.

This is a live candidate. Its tests use ordinary temporary Stores and a copied
synthetic test executable. Dependency parsing, compilation, native runtime
tests, and actual Wasm coupling are separate qualification steps; source review
alone does not pass them. No frozen Linux/Windows evidence is relabeled.
The upstream unsandboxed Windows process implementation has best-effort job
assignment and a PID fallback. This interface does not establish stronger OS
sandbox or descendant-isolation qualification than that implementation.


## Portable helper configuration

The public helper requires `--rust-bin`, `--vs-dev-cmd` and `--capnp-bin`; append these explicit arguments to the historical command examples above. [Configuration and qualification limits](../../../tool/PORTABLE_WINDOWS_QUALIFICATION.md). Historical receipts do not qualify this changed helper or a later source overlay. No helper was run during this publication preparation.
