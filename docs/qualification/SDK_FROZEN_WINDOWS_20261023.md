# Selected Windows frozen SDK regression

Source `b370a18afd1f6c6b1ba995209399d26ff432b9cff663b2b49a3c22a37e604566`
passed the current Windows release build and execution of:

| Suite | Actual methods | Scope |
| --- | ---: | --- |
| sdk_frozen_compat | 9 | Original C/C++/Rust task, transform and basic UI guests |
| sdk_frozen_dependency | 3 | Original dependency guests, approved routing and authorization |
| sdk_transport_frozen | 1 | Six original transport containers, modules and bounded metadata |

The last method prepares package metadata; it does not execute HTTP or service
traffic. Every suite was discovered in the actual compiled binary before
execution, with zero ignored or filtered methods. The 53 manifest-listed frozen
files plus both manifests and both root pins remained unchanged, as did the
complete source inventory. No guest was rebuilt, repackaged or repinned.

Evidence is under
`D:\build_cache\_audit\windows-restriction-lineage-20261022-019\sdk-frozen-windows-001`.
`result.json` binds the source, original inputs, three actual executables and
their original logs. This is a bounded compatibility regression on synthetic
local stores, not full SDK26, G04, SDK_FREEZE, product/third-party qualification,
physical UI or release acceptance. Those gates remain open.
