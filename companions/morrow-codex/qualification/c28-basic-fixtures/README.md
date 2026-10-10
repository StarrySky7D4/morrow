# C28 fixed synthetic fixture inputs

The public harness now selects the independently derived [public-session-r2-v1](public-session-r2-v1/README.md) session fixture, together with the historical proposal/process fixtures. Fixture files are immutable inputs; this directory does not automatically download, build or execute plugins.

The omitted historical `morrow_codex_session_exec_guest_r2.wasm` remains a separate local identity: 427258 bytes, SHA256 `b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e`. Its original data section contains private build paths. Its original filename, SESSION_SHA and qualification history are not reassigned to the new bytes. The old historical session wrapper/checker still accepts only the old hash; the public workflow uses a distinct versioned identity.

The new session fixture is 425912 bytes, SHA256 `cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b`. Its offline build, finite path checks, static calling-interface comparison and one actual seven-step synthetic-session qualification have independent scoped readbacks. The new public integration source and its added identity tests have not yet been compiled or run. Adding the missing fixture input does not itself prove the complete public Git tree builds.

Proposal/process fixtures retain their historical state; they were not rebuilt or requalified here. Native Start, lifecycle cleanup/join, production sandbox, SDK26_G04 and release gates remain open. Dated [Stage18 records](../../../../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md) retain their original compile/wrapper distinction.
