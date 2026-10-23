# Checked startup-directory closure, Windows case032

Source `051fe4cd898c7a5b91bca6de3d3f96099789da714869c35542c05ef1b3f3550e` passed the original 47-step flow and all nine required groups
in clean isolated VM028. Review verified 284 checks, 107 original journal
records, 92 input writes and 3523 original input bytes. Four fresh P/C/R/S images
were compiled from byte-bound snapshots and mechanically bound before execution.
The original case and policy owners closed normally; VM028 is Saved and exactly
four VMs remain registered. Historical Unknown owners remain untouched.

The repair removes implicit File-drop closure from the startup-directory pins.
Command preparation now allocates no directory handles; the original Attempt
holds its command before the first native open. Every raw result is retained
before metadata validation. Normal retirement explicitly checks each original
directory handle close under its existing finite budget. Outstanding pins block
the same original completion and registry retirement. Timeout, close failure and
unwind retain uncertainty and prohibit close replay. This does not expand the
post-login proven-clean refusal path or release any historical Unknown charge.

Ninety-three selected regression methods actually executed. Two use real Windows
directories and observe rename refusal while pinned, then successful rename only
after checked closure, including a failed second open. Close-failure, deadline
and unwind controls are state-machine tests; no actual native CloseHandle failure
is claimed from them. The remaining selected protocol/parent/native regressions
include the typed Cap'n Proto transport; JSON rejection and frame bounds remain.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019` / `cwd-pin-qualification-001`, `roles-025`,
`case032-nine-review/result.json`, `vm-case-diagnostic-032` and
`vm028-case032-saved-state/result.json`. The case tag is `f518f98024954d0b9ae3b14a36faaf7a`.

Before compilation, guarded cleanup removed 6,937 regenerable compiler files
(4,365,797,587 logical bytes). D free space increased by 3,923,402,752 observed
bytes. All 6,796 protected file hashes were rechecked; no source, executable or
VM was deleted. The immutable manifest and result are in `old-compiler-leaves-001`.

Full post-login refusal/partial-output/helper/close-failure coverage and more
than 16 actual clean refusals on one original SDK capacity owner remain open.
Full P2/P3, SDK26/G04/SDK_FREEZE, source and release acceptance remain open/false.
