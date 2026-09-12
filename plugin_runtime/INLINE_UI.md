# Borrowed inline UI sessions

`inline_ui::InlineUi` borrows a prepared package, its host-owned connection, and the existing authoritative `HostRuntime` for each `open` or `event`. It never opens or owns a Store. Run these synchronous, fuel-bounded calls inside the native host process, never on Flutter’s rendering thread.

Create the session with a trusted view name, fresh generation, and timeout. The session pins the package digest and opaque `ConnectionBinding`; another connection for the same package or a different runtime cannot replace it. Keep the managed instance alive for the view lifetime. Closing the view only closes UI admission; the owner controls the instance lifecycle.

`Reply` carries view, generation, revision, last admitted event serial, and either encoded validated document bytes or a typed failure. A rejected event returns `Error` without consuming its serial. An admitted event that traps, times out, produces invalid output, or loses authority returns a failure with its consumed serial and unchanged revision. Callers must not replay it automatically. Use the session getters to report authoritative state after a rejected request.

Only the fixed `ui.form` and `ui.edit` pure transform contracts are accepted. They cannot enter core dispatch, even if their connection was separately granted content capabilities. The execution and completion envelope, zero exit status, absence of content responses and host calls, output type, and bounded UI document are checked before replacing the view. Authority is checked before and after guest execution. A revoked completion closes the UI and is never returned as document bytes.

A document is a view update, never evidence of a saved card. Real business changes must use the unique host’s separately authorized content command path. A timeout or process termination is not a rollback guarantee. Pure loops stop through the configured fuel bound; this serial adapter does not promise immediate cancellation while the host is processing a guest request.

Independent tests in `tests/inline_ui.rs` execute synthetic Wasm through the real runtime: open and edit, trap/fuel/exchange failures, rejected and consumed event serials, wrong connection/runtime, revocation, nonzero exit, and closed views. Product UI wiring, real Rust-built business packages, and process/device qualification remain separate evidence.
