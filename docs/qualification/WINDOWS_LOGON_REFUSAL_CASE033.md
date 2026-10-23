# Actual Windows logon-refusal qualification: case033 retained

Source `ba49c9be6da437abcb5cbed3cea5337de892833b507eada77a9bede789592249`
compiled all four mechanically bound roles and passed 100 selected native
protocol/CWD/refusal regressions. The new controller passed 67 pure profile
checks, the original profile 85, the capture protocols 42 and fixed policy 18.
These preparation results are not actual LogonUser or full P2 acceptance.

Case `2388b6808b4f4f9d9f0da0c10a102940` ran once in VM028. Its first
39 steps returned. Step40, `sealed-qualify-logon-refusal`, reserved its attempt
and sealed Unknown with `original production factory absent`. The actual
LogonUser call and native-start command were not reached. The controller and
parent owners remain retained. The same policy owner conditionally restored
its actual temporary privilege addition: policy_clean=true, case_clean=false.
This is not permission to release or reuse the product scene.

The failure is an input-fixture ownership lookup: native-context previously
consumed the outer factory Option and transferred the exact backend/runtime
to the original Context/Worker. The fixture later inspected the empty outer
Option. The working-tree correction retains a Weak reference when that exact
backend transfers, requires the outer factory to remain consumed, upgrades
only the original live backend for qualification, and drops that temporary
strong reference at the end of the command. It adds no cleanup capability and
must still pass a new independent VM case.

The fixed public diagnostic snapshot contains 89 contiguous journal records;
it was copied without contacting the original execution owner. Evidence is
`D:\build_cache\_audit\windows-restriction-lineage-20261022-019` /
`case033-summary.json` and `vm-case-diagnostic-033`. Exec session20924 and
VM028 must not be polled, adopted, replayed or terminated. The previously
prepared normal case034 was not executed and its clean-case guards now fail.

Four VMs remain registered. There is no currently verified clean reusable VM.
The purpose of the unrelated offline VM is pending user clarification before
any registration reclamation; its disk remains untouched. Windows SDK loopback
qualification can proceed independently. Full P2/P3/Windows SDK26/G04/FREEZE,
source acceptance and release eligibility remain open/false.

Later update: [cases035 and036](WINDOWS_LOGON_AND_NORMAL_CASE035_036.md) passed in a fresh isolated VM. VM028 was subsequently found Off and its registration was archived with disks retained. The case033 Unknown outcome and original owners remain unchanged.
