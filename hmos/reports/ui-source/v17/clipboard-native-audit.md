# dev17 clipboard native adapter — bounded local evidence

Captured 2026-10-07. This report covers the new Rust parser and FD bridge, not final HAP/device clipboard acceptance. The full Flutter parity goal remains **OPEN**. Exact source inventory, toolchain, reference files and archive hashes are recorded in `clipboard-native-inputs.json`.

## Implemented boundary

`clipboardConvert(request, sourceFd)` reads the immutable private spool, verifies its complete original SHA256, and returns normalized text plus bounded embedded-image metadata. `clipboardImage(request, sourceFd, destinationFd, maxBytes)` reparses and verifies that same source and the requested local image ID before truncating/writing the private destination. Neither API reads paths/URIs, fetches remote images, mutates Store, executes active content, nor creates a capture ticket.

Requests are strict JSON: `format` (`html`, `rtf`, `xml`, `plain`), `section` (`description`, `title`, `todos`, `hypothesis`, `conclusion`) and lowercase `expected_sha256`; image extraction additionally requires `local_id`. Unknown fields are rejected. Convert replies include `ok`, `error`, `paste_text`, `warnings`, `images`, `source_sha256` and decimal-string `source_byte_length`. Image replies reuse `FileReply`. The existing 512 KiB native request cap is unchanged; source/image bytes do not cross JSON.

The parser delegates conversion to the frozen shared `capture::{plain, html, spreadsheet, rtf}` code. HTML uses a real HTML5 DOM and removes active subtrees before conversion. XML is namespace-aware and rejects DTDs/external entities. RTF retains original ANSI bytes/code pages separately from UTF8/UTF16 text flavors; malformed/unsupported encoding fails explicitly. UTF8 and BOM-marked UTF16LE/BE text are strict, and hashes always cover original bytes. HTML data images are decoded, MIME/signature checked, hashed and named deterministically; remote/local image links are not opened. Source-provided `attachment:` aliases cannot impersonate generated local images.

Budgets fail explicitly rather than returning a truncated success: source 2 MiB **bytes**, 1,024 DOM nodes, depth 40, output 20,000 UTF16 units, 10 HTML images, 64 MiB image total, 8 Spreadsheet XML tables, 500 table rows and 80 effective columns. HTML node/depth checks occur during incremental parsing. Images are reparsed for every extraction, and wrong source hashes, unknown IDs or insufficient destination limits are rejected before destination mutation. Duplicate FDs and source/destination same-inode aliases are rejected. NAPI duplicates authorized FDs before queueing; existing worker/destructor ownership closes them on failure or transfers them once to Rust.

## Recorded validation

| Check | Result | Evidence |
| --- | --- | --- |
| Full Windows Rust library suite | 91 passed, 0 failed, 1 conditional comparison ignored | `clipboard-rust-tests.log` |
| Actual Flutter reference capture | 1 test passed; 5 full outputs captured | `clipboard-flutter-tests.log`, `clipboard-flutter-reference_test.dart`, `clipboard-flutter-reference.json` |
| Conditional Rust/Flutter comparison run separately | 1 passed; all 5 complete outputs exactly matched | `clipboard-flutter-rust-compare.log` |
| OHOS ARM64 release staticlib | PASS | `clipboard-arm64-build.log` |
| OHOS x64 release staticlib | PASS | `clipboard-x64-build.log` |
| C++ syntax against SDK, both ABIs | exit 0 | `clipboard-cpp-syntax.log` |
| New Unix FD runtime / actual pasteboard grants / device rich paste | NOT_RUN in this subtask | Parent owns HAP/device acceptance |

The five Flutter comparisons execute the actual `build/io-safety-refactor/lib/content/rich_content.dart` package at reference HEAD `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`: Word-style HTML formatting/unsafe links, an HTML rowspan table, Spreadsheet XML with formula text, RTF Unicode/hidden object content, and multiline TSV. The reference source/test/pubspec files were clean in that checkout and are hash-bound by the manifest. The shared Rust snapshot remains the existing isolated snapshot; it was not refreshed or edited to manufacture the comparison.

New parser tests cover malformed HTML/entity handling, active subtree pruning, forbidden attachment aliases, namespace confusion and DTDs, sparse/formula/merged table behavior, malformed table spans/limits, raw RTF/code pages/font charsets/Unicode, strict UTF16, image metadata and extraction, whole-list image failure, source/ID/hash/limit rejection before writes, schema/output/node/depth budgets, short/interrupted reads and failing writers.

Final archives copied by `scripts/build-rust.ps1` before parent HAP build:

| Archive | Bytes | SHA256 |
| --- | ---: | --- |
| `entry/src/main/cpp/rust/arm64-v8a/libmorrow_hmos.a` | 55,049,942 | `BF6966CF04247C552A82CE6E93220734059AA3AA326F5DB448C97BEEFA64954F` |
| `entry/src/main/cpp/rust/x86_64/libmorrow_hmos.a` | 53,467,100 | `E64961D0AA087B50BB0FBA331BC47036A369C02A3F4666717B0E5C5330B98606` |

Both archives correspond to the same final frozen clipboard/native source inventory. Builds used locked/offline Cargo release targets `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos`, Rust 1.95.0 and SDK clang 15.0.4. The source checkout was `codex/ArkTsUI` at base HEAD `da284f5043a42124a507f52226056cc29d7408d2` plus uncommitted dev17 work; a base commit alone does not identify this build.

## Practical limits kept open

The 2 MiB byte source cap is stricter than Flutter's 2 Mi UTF16-unit source cap. The five local converter comparisons do not prove the full Office/pasteboard provider matrix, capture lineage/tickets, HUKS/protected storage, XLSX/DOCX/OLE package import, or complete Flutter semantics. XML support here is the Spreadsheet clipboard text flavor. Embedded-image signature validation is not a pixel decoder or proof of device image rendering. Unsupported RTF charset/binary forms are explicit errors; intentional U+FFFD text is conservatively rejected. Existing field limits/grapheme behavior, editor rollback, URI authorization and persistent attachment identity belong to the caller and require their own tests/device evidence.

Reproduce the saved Flutter fixture capture from the reference Flutter checkout by setting `HMOS_CLIPBOARD_FLUTTER_REFERENCE` to a test output JSON and running `flutter test --no-pub <absolute clipboard-flutter-reference_test.dart> --reporter expanded`. Then point that variable at the captured JSON and run `cargo test --locked --offline --manifest-path hmos/rust/Cargo.toml --lib clipboard::tests::actual_flutter_reference_outputs_match_five_complete_fixtures -- --ignored`. The default full suite intentionally leaves this environment-dependent comparison ignored.
