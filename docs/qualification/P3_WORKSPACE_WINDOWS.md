# Two actual Windows workspaces during draft handoff

Source `b8d8095bc2d39f15efce5327b18faa426ebaabf267d839f889ec41aacf5a93a1` passed one visible native Windows client, PID20984, with
unchanged before/after source inventory and an installed kernel matching the
assembled image. The test uses framework-injected controls in the actual
MorrowApp and two actual Rust workbenches with separate synthetic libraries.

Library A starts with an original persisted handoff and is reopened before UI
recovery. The formal recovery UI explicitly completes the child. Both original
parent and child remain active while the parent-retirement confirmation is open.
Switching the product to library B removes both old recovery routes. A's parent
remains active; B has no cards or foreign draft. Returning to A requires a new
explicit inspection and confirmation before its original parent retires. The
child's exact raw request bytes, original parent-child link and unchanged business
revision survive another host reopen. B remains empty. Both original transports
close normally after the successful scenario.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-workspace-windows-001` and `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-workspace-summary.json`. This does not
qualify physical mouse/keyboard/IME, a native library picker, application-window
shutdown or all P3 failure combinations. The historical Unknown owners remain
untouched. Full P2/P3/SDK/source/release acceptance is still open/false.
