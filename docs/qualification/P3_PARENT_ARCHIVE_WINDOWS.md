# Formal Windows parent archive and import cleanup

Nine native Windows clients passed on the same frozen source `3de6d36972c40752f978cd5ec019b6f5a0c62e732e659f2fae546d201aed6144`:

- A formal attachment reselection and business Save, a separate history/archive
  client, then an independent restart reader. Old attachment references and old
  generation exports are rejected; selecting the same path again produces a new
  import identity. Deleting the input file before Save does not lose its bytes.
- A client deliberately withholds the real parent-import cleanup reply after the
  host has completed it. The formal Save recovery inspects the original outcome,
  preserves newer raw S2 input, and does not replay business or cleanup. The real
  host is reopened and its exact original bytes and commit evidence verified.
- Five further independent clients verify original lost business, Prepare,
  Complete, child-write and Retire replies through the updated completion flow.
  Each performs one actual business submission, one Prepare/Complete/Retire and
  one acknowledgement; newer S2 and original attachments survive real host
  reopen, and late parent writes remain rejected. Import cleanup counters in
  those five fixture results refer only to the unused fault wrapper, not to
  production cleanup dispatch counts.

The retained original cleanup was dispatched once and inspected once; Prepare,
Complete, Retire and business submission each occurred once. Parent retirement,
immutable archive identity, child bytes and business revision survive restart.

Two product repairs were required. Archive admission now verifies the original
handoff's ParentRetired status instead of treating every noncancelled proposal
as unfinished. The original proposal, operation reservations and lineage remain
intact. Formal commit completion now reconciles its own retired parent's import
owner before acknowledgement and capture close. An ambiguous cleanup future is
retained; explicit recovery only inspects the exact retired generation and empty
original import list. Archive still rejects pending imports and live capture.

Attempts001/002 remain retained: the first reproduced the completed-proposal
archive refusal; the second correctly refused archive because original imports
had not retired. Neither failed store was reused for the successful run.

Separately, 28 actual host draft tests and 12 handoff tests passed with no ignored
or filtered methods on source `2f45728241f20a3052643a5223fad5d49a1494d154c1f7e919e2254b1cffd558`. This includes actual 256-history
and 16-active boundaries, restart, import/capture closure, completed-proposal
archive, late-parent rejection and competing-successor rejection. That earlier
source snapshot precedes the Dart import-cleanup changes; the two qualifications
are not relabelled as one source or combined full-product acceptance.

Original evidence and exact executable/source pins:
`D:\build_cache\_audit\windows-restriction-lineage-20261022-019` / `p3-parent-archive-summary.json`, `p3-receipt-regression-summary.json`
and `p3-archive-host-001/result.json`.
The earlier standalone reselection result remains in `p3-reselection-summary.json`.

Input is framework-injected in actual Windows app windows. Physical input,
native file-picker interaction and rendered-pixel acceptance are not claimed.
Full P3 remains open, including long-term import/decision/proposal capacity,
the complete fault matrix and final combined-source qualification. SDK26, G04,
SDK_FREEZE, source acceptance and release eligibility remain open/false.
