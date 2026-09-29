# CC Switch conversion candidate review

Source: `farion1231/cc-switch` commit
`846de29c13ac4d65f164db8c15dd5fd58e29f972`.
The eleven files under `upstream/reference/cc-switch/` are exact fetched file
snapshots, not a complete checkout. `receipts/upstream-cc-switch-connector.json`
and `receipts/upstream-cc-switch-local-deps.json` record fixed URLs and Git blob
SHA-1 identities. All eleven local `git hash-object` values matched the API.
The original MIT `LICENSE` is retained. No source modifications or runtime tests
have been applied to these snapshots.

## Candidate module closure

Paths below are relative to `src-tauri/src/`. This is a source-level extraction
review, not Cargo-resolved or compiled native/Wasm closure evidence.

| Candidate | Actual local dependencies | Disposition |
|---|---|---|
| `proxy/providers/transform_codex_chat.rs` | `codex_chat_common`, `provider::CodexChatReasoningConfig`, `proxy::{error,json_canonical,tool_media}`; serde_json; std collections | Retain reviewed conversion portions with explicit unsupported-feature errors; replace permissive completion inference |
| `proxy/providers/streaming_codex_chat.rs` | `codex_responses_sse`, `codex_chat_common`, `transform_codex_chat`, `json_canonical`, `sse`; bytes/futures/serde_json/async-stream/tokio::pin | Use as reference for state/call association; replace framing, error, terminal and budget behavior before claiming compatibility |
| `proxy/providers/codex_chat_common.rs` | serde_json | Retain provider-supplied field handling only; exclude inference from literal think tags |
| `proxy/providers/codex_responses_sse.rs` | bytes, serde_json | Candidate event serialization helpers; validate independent expected vectors |
| `proxy/json_canonical.rs` | serde_json, sha2 | Candidate deterministic functions; preserve original tool-argument bytes separately from canonical form |
| `proxy/sse.rs` | std string/bytes | Review framing/UTF-8 helpers against required BOM, CR/LF, EOF and bounded-state semantics |
| `proxy/tool_media.rs` | json_canonical, serde_json | Explicitly reject unsupported media in first supported subset; do not silently move or remove required media |
| `proxy/error.rs` | axum response conversion, reqwest error categorization, serde_json, thiserror | Replace with portable conversion errors; do not import this whole server error module into Wasm |
| `provider.rs` | http/indexmap/serde/serde_json plus app_config, codex_config, grok_config and pi_config | Extract only the required pure reasoning configuration type at line 403; exclude complete provider/configuration module |
| `src-tauri/Cargo.toml` | Tauri/build plugin graph, HTTP server/client, SQLite, key/platform/desktop integrations | Exclude the application crate as a plugin dependency; create a minimal conversion crate only when implementing P-05 |

The application manifest has a `tauri-build` build dependency, reqwest socket
features, hyper `full`, rusqlite `bundled/backup/hooks`, and platform-specific
dependencies including webkit2gtk, windows-sys and objc2. Copying the whole crate
would defeat a pure Wasm dependency boundary. No native or Wasm plugin Cargo.lock
has been created just to make this table appear compiled.

## Concrete semantic differences requiring replacement

- `streaming_codex_chat.rs:849` calls finalization on `[DONE]` without requiring
  the project's independent terminal validation.
- `streaming_codex_chat.rs:858` silently continues on invalid JSON; qualification
  must reject malformed data instead of losing a potentially required event.
- `streaming_codex_chat.rs:890` handles EOF with substantive output but no finish
  reason by setting `finish_reason = "length"`; preserve actual upstream facts
  and distinguish truncated transport from an observed provider finish reason.
- `transform_codex_chat.rs:2064` maps every finish reason other than `length` to
  `completed`, including absent/unknown values. The planned adapter must validate
  recognized terminal conditions and cannot manufacture success.
- `codex_chat_common.rs:211` and streaming `InlineThinkMode` split text using think
  tags. The v1.1 plan forbids inferring hidden reasoning from text patterns. These
  paths must be excluded/replaced even though they exist in the fixed upstream.

These findings are static source facts and migration decisions. They are not
claims that the upstream is universally incorrect for its own product contract.
P-05 must supply separate native/Wasm vectors for the plugin's supported subset.
No model/network request, account access, or CC Switch configuration was used.
