# Windows draft disk-full recovery

Two independent native Windows clients passed on source
`894ada3bde48ac8cbf16199ce04ddf1045233402ed86db17c005482d6fabbb86`,
commit `f8ef33ec091a82d68f3c2745dde950f8762d8878` plus the recorded working tree.
The complete source manifests before and after execution are identical.

The formal NewIdeaDialog now offers explicit draft-write inspection and retry.
Both actions require the current original owner/view lease and an unresolved
frozen draft request. They do not continue business Save. A failed commit may
use these actions only while still at its parent draft stage; later commit
stages retain their separate recovery protocol. The labels cover all nine
configured UI languages.

The fixture creates a fresh 256 MiB VHDX under `D:\build_cache\_audit`, pins its
disk and volume identities, and mounts it in its own directory without a drive
letter. It checkpoints the isolated database before exhausting this volume by
actual writes. Windows returned ERROR_DISK_FULL 112 and available space was zero.
The production host correctly reported `CommitUnknown` at its SQLite commit
boundary; this was not interpreted as proven rollback.

The first client verified the previously confirmed S1 record unchanged, retained
the frozen S2 request, and accepted newer live S3 input. Formal inspection did
not clear Unknown or submit a business operation. After deleting only the exact
identity-checked filler file, formal retry returned the original S2 operation
and request digest while preserving S3, including selection/composition state.
A separate formal Save submitted S3 once. The second client reopened the actual
store and verified one card at revision 1, the exact business operation evidence
and the final description digest. Both clients completed their original close
sequence. The successful virtual volume was detached and its VHDX retained.

The first failed run expected a direct disk-full error string where the host
correctly returned CommitUnknown. The second stopped on a test framework guard
conflict in the volume helper's asynchronous assertion. The helper now reports
failure through its future, outside guarded tester APIs. Neither failed run was
replayed or overwritten; every run used a new volume and store.

Evidence root:
`D:\build_cache\_audit\windows-restriction-lineage-20261022-019`.
The bound summary is `p3-diskfull-summary.json`; actual client/source/artifact
records are under `p3-diskfull-windows-003`, and volume identity, error112,
release and detach records are under `p3-disk-volume-003`. Failed runs001/002
and their original volumes remain retained. Four VMs remain registered.

This proves the selected formal disk-full/recovery/restart scenario with
framework-injected input and real native host/database execution. It does not
prove physical keyboard/IME or rendered pixels, sudden power loss, all P3 fault
and maintenance cases, guest source qualification, full SDK or release readiness.
The workbench/theme guest packages were reused with their original exact hashes;
they were not rebuilt. P2/P3, SDK26/G04/FREEZE and source/release acceptance remain
open. Cases030/031 are separately bound to their earlier native source snapshot.
