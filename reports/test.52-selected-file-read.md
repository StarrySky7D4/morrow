# test.52 selected-file read (W2 host broker)

Date: 2026-09-15
Branch: `track-a/w1-io-contract` (not merged)

## Done

- Host copies a selected file into a private spool; the original path is discarded
- `ResourceRef` is a host token bound to host/connection/generation/package digest
- 64 KiB chunks, explicit EOF, cancel, offset overflow, revoke, cross-connection deny
- Non-empty paths on selected-file reads return InvalidPath
- Unapproved kinds do not become silent success

## Not done

- `morrow_io_v1.call` is still rejected by the Wasm runner
- No C/C++/Rust guest example through the workbench
- No Manager/Registry IO approval persistence
- No HTTP GET
- Guessing a token still cannot read another connection's spool

## Limits

This package proves the host broker, not a complete SDK file API.
