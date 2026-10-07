# dev18 clipboard source capacity — native and final integration evidence

Captured 2026-10-07. This subtask implements and builds the native source-capacity candidate, then reads the parent's final SDK/build evidence and independently verifies the final HAP file identity. It performs no device operation, Git mutation, HAP install or capture-ticket creation. Parent owns SDK integration, packaging and device acceptance. The full Flutter parity goal remains **OPEN**.

## Source-grounded scope

Fresh source reads show these distinct reference routes:

- `lib/attachments/clipboard_import.dart:165` limits ordinary text by Dart `String.length` to 2 × 1,024 × 1,024 UTF16 units. HTML/XML pure converters in `lib/content/rich_content.dart:50,214` use that same unit limit.
- The `convertRtf` closure at `clipboard_import.dart:107–110` calls the pure `rtfToPlainText` only when `plugin == null`. That helper's limit at `rich_content.dart:276` is 8 × 1,024 × 1,024 UTF16 units. RTF-only clipboard and Office items call this closure at lines 262 and 315.
- With a plugin, the closure directly awaits `plugin.capture`; the item/Office catches at lines 270 and 323 report warnings. They do not fall back to the 8Mi helper after plugin failure. The actual `studio_native.dart → captureWithPlugin` path has a uniform 2Mi source-unit check at `capture_native.dart:22` and a 65,536-byte serialized request check at line 143.
- `main.dart:5608` passes `_plugin`; its getter at line 5281 uses `_editor?.studio ?? widget.plugin`. The optional `StudioBackend? plugin` at line 5174 makes the no-plugin route reachable, while a native studio editor supplies a plugin. No `_toRtfCaptured` or `_normalize` functions were found in the current reference implementations.
- `office_clipboard.dart` transports the Windows method-channel result with a 30-second timeout. Original downloadable files/RTF and aggregate files use the attachment budget in `readPaste`, not a 2Mi source-unit cap. HMOS keeps its existing stricter 64MiB spool/import budget.

Therefore dev18 aligns plain/HTML/XML with their real 2Mi UTF16 source capacity and RTF with the reachable **no-plugin local** 8Mi capacity. It does not claim equality with the plugin transport, protected capture authority, or the entire Office provider matrix. Exact reference files and hashes are bound in `clipboard-native-inputs.json`.

## Native behavior

| Format | Decoded original source limit | Maximum input byte envelope |
| --- | ---: | ---: |
| plain / HTML / XML | 2,097,152 UTF16 units | 6,291,459 bytes |
| RTF | 8,388,608 UTF16 units | 25,165,827 bytes |

The envelopes are `3 × units + 3`: every valid UTF8 scalar uses at most three bytes per UTF16 unit, plus an optional UTF8 BOM. Strict BOM-marked UTF16 payloads fit the same envelope. A byte envelope only bounds FD reads; it is not a proxy for the semantic source-unit limit.

The native adapter reads at most envelope + one sentinel byte and rejects an oversized source explicitly. A within-envelope source is fully read and verified against the caller's original SHA256 before conversion. Strict text decoding checks UTF16 units before HTML/XML DOM or plain conversion. UTF16 BOM input checks exact payload units before allocation; malformed UTF8, odd UTF16 byte counts, unpaired UTF16 surrogates and NUL remain explicit errors. No input is cropped or silently repaired.

Raw RTF requires lexical code-page/font/group handling. Original source units are counted from exact original byte spans decoded with their actual active code page, including the original ASCII spellings of controls and escaped hex. Generated normalized escapes and final text are not used to measure source size. The check completes before shared `capture::rtf` conversion. UTF8/UTF16 BOM RTF uses strict Unicode literal text while ANSI hex runs retain their declared code page; this supports complete Unicode original-source capacity without treating literal Unicode bytes as ANSI. Original BOM/bytes remain in the source SHA and byte length. Large multibyte runs flush only at complete encoding boundaries.

SDK Unicode RTF strings need an unambiguous Unicode serialization marker when their RTF declaration still names an ANSI code page. The final authorized ETS source serializes string-valued RTF deterministically with UTF8 BOM; native supports that marker. The envelope's three BOM bytes and original SHA/byte length include the complete serialized source. Raw ArrayBuffer items retain their exact original bytes and declared ANSI/font encoding. Fresh final source reads and the parent's final model logs verify this serialization, the complete SHA/length contract, and charging the three marker bytes against the unchanged shared spool budget. Unpaired SDK UTF16 surrogates are rejected before encoding; valid pairs and intentional U+FFFD are preserved at the original-read layer. The identified SDK-string encoding ambiguity is fixed in this final source/build candidate; device acceptance remains separate.

The API schema and C++/DTS ABI are unchanged. Request strings remain capped at 512KiB; output remains 20,000 UTF16 units; DOM nodes/depth, table dimensions, image count/total, spool/Store bytes, slot count and SHA/FD ownership boundaries are unchanged. No ticket/lineage or attachment identity is fabricated. Image extraction continues to recheck full original source, image ID and destination limit before destination mutation.

## Validation and preserved artifacts

| Check | Result | Evidence |
| --- | --- | --- |
| Final full Windows Rust suite | 98 passed, 0 failed, 2 environment-dependent comparisons ignored | `clipboard-rust-tests.log` |
| Existing five complete Flutter outputs, separately rerun on final source | 1 passed; all five outputs unchanged | `clipboard-five-fixture-rust-compare.log` |
| Fresh actual Flutter capacity capture | 1 passed; six complete source identities/outcomes | `clipboard-capacity-flutter-tests.log`, `clipboard-capacity-reference_test.dart`, `clipboard-capacity-reference.json` |
| Rust comparison against those six Flutter identities | 1 passed; source unit/byte counts, SHA, accepted/rejected outcomes and successful outputs match | `clipboard-capacity-rust-compare.log` |
| Final OHOS ARM64/x64 release candidates | both PASS | `clipboard-arm64-build.log`, `clipboard-x64-build.log` |
| Final SDK model matrix | 464 passed, 0 failed, 0 skipped | `arkts-final-model-tests.log`; clipboard input 55, clipboard/files 59, attachment/files 97 are included |
| Final dev18 HAP build | PASS in 6.782 s; unsigned | `hap-release-build.log`; signing was skipped because no signing configuration was present |
| Final dev18 device capacity / native FD runtime / image gestures | NOT_RUN | Parent owns subsequent device acceptance; dev17 runtime evidence does not qualify dev18 |

The six Flutter cases are HTML and XML at 2Mi units and one unit above, plus the reachable pure RTF fallback at 8Mi units and one unit above. Fixtures generate full multibyte sources and record their complete identities; multi-megabyte source blobs are not checked into reports. Additional Rust tests cover exact UTF8/UTF16 BOM envelopes, supplementary characters counting as two units, full raw GBK/BOM RTF capacity, original hex-control spelling counts, strict encoding domains, source overflows and unchanged output/image/hash failure behavior. The full debug suite's 8Mi raw-codepage stress fixture took part in a 98.59-second suite; that duration is host test evidence, not a device latency result.

Candidate builds use `clipboard-native-build.ps1` with locked/offline Cargo and the existing SDK. The script archives candidates separately and never copies over the published `entry/src/main/cpp/rust` inputs. Parent subsequently adopted the dev18 candidates for final packaging and owns the packaged-native comparison.

The final unsigned HAP at `hmos/entry/build/default/outputs/default/entry-default-unsigned.hap` was independently read after the final build: **27,866,016 bytes**, SHA256 **`910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2`**. This identity matches the parent's final integration record. A successful unsigned package and model matrix are source/build-qualified delivery evidence; no dev18 installation, system clipboard permission, runtime capacity or rendered-image acceptance is claimed.

| Candidate archive | Bytes | SHA256 |
| --- | ---: | --- |
| `hmos/.build/clipboard-native/dev18/arm64-v8a/libmorrow_hmos.a` | 55,053,456 | `8EA7F0446E46BE185519E50357691065724FC6061C37E2B269A9631E97DB8D89` |
| `hmos/.build/clipboard-native/dev18/x86_64/libmorrow_hmos.a` | 53,471,386 | `E241203FF46BB7C7AAFDF38FDE60EF8119E694525629A7377A585659D9F7FE44` |

The published dev17 archives were first verified and independently preserved at `hmos/.build/clipboard-native/dev17-preserved/{abi}/libmorrow_hmos.a`. Their hashes remain ARM64 `BF6966CF04247C552A82CE6E93220734059AA3AA326F5DB448C97BEEFA64954F` and x64 `E64961D0AA087B50BB0FBA331BC47036A369C02A3F4666717B0E5C5330B98606`. Both original `cpp/rust` copies still had these hashes immediately after candidate build; a subsequent parent-authorized adoption is a separate step. The manifest binds 253 native source/build inputs, both final candidates and both preserved dev17 archives.

## Explicit gaps

HMOS strict decoding intentionally differs from Flutter `decodeClipboardText`, which also uses an unmarked UTF16LE heuristic, allows malformed UTF8 replacement and removes NUL. Unsupported RTF binary/charset cases stay errors. The existing native RTF-output U+FFFD guard conservatively rejects even intentional U+FFFD; ordinary text/HTML/XML and the SDK original-read layer do not blanket-reject that valid character. This round does not resolve that older RTF semantic gap. Successful local source-capacity checks do not prove device pasteboard grants, Office provider interoperability, actual image pixel rendering, HUKS/protected capture lineage, capture tickets or complete Flutter product acceptance.
