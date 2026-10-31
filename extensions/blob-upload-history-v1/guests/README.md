# Wasm qualification consumers

These Rust, C and C++17 programs exercise the actual history decoder through
the unchanged task-v1 import and correlated task completion. They accept only
the fixed synthetic selection `[1; 32]`, query nonce `[9; 32]` and qualification
records. They are deliberately not a general product delivery route or a grant
to query history. Production callers must supply their separately selected
correlations and current disclosure approval.

The fixture checks full-width counters, signed values, Unicode, borrowed views,
and original Unknown independently from current Staged bytes. Invalid input
produces an ordinary correlated task failure; it never uploads, acknowledges,
queries a database, retries an earlier task, or repairs an original owner.
C++ additionally checks that moving the packet transfers its view lifetime.

The build uses the existing unchanged SDK task codecs. Only the two task imports
are allowed; no WASI filesystem, clock, network, content or new history import
is expected. Qualification results must record actual execution separately from
compilation. This directory is outside the earlier native-only source package.
