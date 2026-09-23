# Late content reply protection (2026-09-23)

The native adapter now treats a successful mutation or captured editor save reply as a commit receipt. It reads the current card before returning an `Idea`. The returned UI value carries an unsigned `u64` revision, current deleted state, and whether the receipt was historical. These fields are session metadata and are omitted from `Idea.toJson()` and the authoritative card body. A confirmed commit followed by a failed current read raises `WorkbenchCommittedRefreshFailure`; the editor retains its original operation and payload. The UI offers a read-only refresh for that failure and for generic native mutation failures, never a fresh business operation from the retry button.

The adapter remembers revisions monotonically after attachment materialization. `load()` reconciles IDs and versions observed during its page walk, with three bounded read-only passes and a final synchronous check before returning. The Studio keeps unsigned revision knowledge, ignores older results and old-workspace replies, and applies delete only when the current snapshot is deleted. On a backend switch, it clears the previous workspace's visible cards without persisting them, then loads the new backend's cards. A refresh with a missing known live ID or stale version is retried rather than published.

Validation completed in this worktree:

- `flutter analyze --no-pub` on the three production files and two directly related tests: no issues.
- `flutter test --no-pub test/workbench_late_content_test.dart`: 3 passed, covering signed `u64`, workspace switch with overlapping card ID, incomplete refresh, historical delete, late old revision, and restored initial revision.
- `flutter test --no-pub test/workbench_ids_native_test.dart test/editor_capture_native_test.dart` with `build/workbench-host/release/morrow-workbench-host.exe` and `build/workbench-host/bundle/workbench.morrowplugin`: 5 passed. The captured editor test saves once, commits a newer edit through another real editor session, then retries the old editor operation and verifies the current content and historical-receipt marker.

The page-scan race is protected by bounded reconciliation and a fake-widget incomplete-refresh test. No native transport fault injection was run for an ID appearing exactly during a page scan. These checks establish behavior for the tested Windows host bundle and Dart UI paths; the parent's new Windows build provides separate combined validation for the final host artifacts.

The widget identity test reuses one `MorrowApp` State with one `MemoryStorage` while changing its backend. It proves late callbacks and previously visible A cards do not enter B's UI or get persisted during an incomplete B refresh. Replacing `MorrowApp.widget.storage` and its backend together within that same State is not supported or verified here: `_MorrowAppState.storage` is initialized once. The Windows application session flow shows Recovery between sessions and builds a new `MorrowApp` State for the next library; that lifecycle still needs the parent's combined run for current-artifact evidence.

## Final combined validation

The parent completed a Windows x64 Release build in `build/windows-corners/x64` after the production changes settled. The rebuilt host and plugin passed 14 tests across preferences reconciliation, Rust workbench integration, captured editors and persisted query IDs. The related Flutter regression combination passed 33 tests across late content replies, editor capture, workspace lifecycle, query coordination and the main widget suite. These include earlier directed tests; counts are not additive to those earlier runs.

Logs: `build/review-versioned-content-windows.log`, `build/review-late-content-native.log`, `build/review-late-content-flutter.log`. Nine-language commit-confirmed/refresh-failed messages were generated from authoritative ARB fragments; resource consistency and seven catalog tests passed. Current-artifact verification did not include a real Windows window interaction run, native page-scan fault injection, Android or browser qualification. No commit, push or release was performed.
