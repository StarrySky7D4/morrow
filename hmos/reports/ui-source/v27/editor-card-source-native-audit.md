# v27 ordinary current-card source classification

Status: **PASS_SCOPED**, native current-card DTO foundation only. Index integration,
OHOS builds/transport, device rendering, and full Flutter product parity are
outside this worker's qualification. No Git, SDK/NDK adoption, or device action
was performed by this worker. v25/v26 evidence remains unchanged.

## Contract and authoritative source

Ordinary `Reply.cards[]` now includes `content_kind` only after reading the true
current `CardRecord` and successfully validating all of its schema2 properties:

| Value | Meaning | Current editor mode |
| --- | --- | --- |
| `v2` | Actual idea format2, complete `tasks_v2::decode` succeeds, no migration Origin | `current_v2` |
| `legacy` | Actual idea format2, complete decode succeeds, including validated V1 migration Origin | `current_v2` |

`legacy` describes validated migration provenance inside a genuine current V2
record. It does not expose legacy LF editing, supply a continuation root, or
grant ownership from historical editor markers. Native TaskIds, task counts,
completion values, marker bytes, and TaskId prefixes do not determine the kind.

The decoder checks the Origin card identity, source revision and digest,
migrator/target versions, original V1 properties/title, deterministic mapping,
and retained task provenance. A malformed body/version/Origin rejects the whole
ordinary projection. Unknown outer card types also reject it. Ordinary format1
was already rejected with `UnsupportedVersion`; there is no new migration or
read fallback. Valid migration sources are not rejected merely for their Origin.

The normal projection retains the exact current full `source` and decimal
`revision`. Classification itself is read only. Core CAS, fixed wire journals,
Unknown handling, quotas, pins, and all business transaction implementations are
unchanged. Classification does not prove a historical business operation.

`CardView` uses an internal optional field so strict
`editor_commit.historical_card` keeps its existing exact 14 keys: `id`,
`revision`, `source`, `title`, `description`, `hypothesis`, `conclusion`,
`category`, `stage`, `favorite`, `deleted`, `deleted_at`, `tasks`, and `assets`.
Historical constructors explicitly omit `content_kind`. DTO tests assert the
exact key set on save, inspect, and reopened exact retry.

## Actual Flutter reference

The actual frozen Flutter `build/win-cloud-20261005/lib/main.dart`
(SHA256 `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06`)
lines 4981–5003 reads the latest actual versioned record before opening a current
versioned editor. The adapter
`build/win-cloud-20261005/lib/plugins/versioned_editor_adapter.dart`
(SHA256 `104E82C2573C973F8C6745010E22826BD144E1B99220A51151807B42915A8D16`)
lines 143–162 requires matching id/revision/format2, unchanged category/stage/
favorite, and empty legacy todos/completed/editor.todos. Thus both valid kinds
use the existing task-preserving `current_v2` native contract when reopened
normally. Root owns the ordinary UI entry point. Old owned LF is not assigned a
new baseline through this classification; S2/S3 active Session continuation is
a separate existing contract.

## Fresh evidence for these exact sources

`editor-handoff-native-inputs.json` and its byte-identical `-after.json` list all
four changed/new native source/test inputs with repository paths, bytes and
SHA256. Post-suite comparison found zero drift. Manifest SHA256:
`BFC532932ED3C43E06ED3924C1897CC49FCBEA7743511CDEF055A392AF76BEEB`.

- Focused current-source Store suite: **4 PASS, 0 failures, 1 explicitly ignored
  exporter**, 1.94s, process exit0 (`card-source-focused-stage1.log`).
- Exact exporter: **1 PASS**, 0.52s, exit0 (`card-source-dto-final.log`).
- Fresh complete default command: **191 library PASS, 0 failures, 17 conditional
  ignored**, 82.34s; **3 attachment binary PASS**, 6.56s; self-check and doc tests
  exit0, 0 tests (`rust-tests-final.log`). Whole command exit0.

The new Store tests exercise a real V1 card, a baseline-bound Core
`ContentMigration` and reopen, rather than merely attaching a manufactured
Origin. A native V2 has migration-looking TaskIds but no Origin. Both cases
retain classification after favorite/category edits, TaskId rename/completion/
reorder/retirement and a genuinely changed current title/body. Current-body
save preserves task IDs/completion/order/retired IDs and Origin, then exact
retry after Store reopen returns the original receipt without another write.
An empty native V2 and a current editor marker stay V2. Eight malformed or
unqualified type/version/body/provenance cases reject without mutation, also
after reopen. Classification cannot supply a raw-LF continuation root.

The complete actual Store fixture is `editor-card-source-store-fixture.json`,
84863 bytes, SHA256
`409C9ADF3FA6297AA60F9EA7130B8D39C451B37516965DA48D7C2548895449FF`.
It contains two cases (`v2`, `legacy`), each with complete initial and six
mutation replies, full pre-save source, original and issued fixed wires,
publication/prepare/issue/read replies, save/current/reopened/inspect/retry
replies. Ordinary cards contain the new classification; strict historical DTOs
omit it. Records and text are synthetic test data in temporary development
stores; this fixture does not contain user library content or credentials.

Crash replay was **NOT_RUN** in v27: the change affects current read projection
and serialized DTOs, not any transaction implementation. Prior v25/v26 crash
proof is historical and is not relabeled as testing this source hash.

## Remaining scope

Root is integrating and qualifying ordinary current-source reopen and active
child restart recovery. This worker does not claim its local tests qualify
native cross-ABI artifacts, SDK/device transport, UI rendering, release
eligibility, or complete Flutter acceptance. GitHub push state is owned and
reported by Root.
