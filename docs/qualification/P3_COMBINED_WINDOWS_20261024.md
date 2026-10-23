# Windows editor combined source verification

The combined run passed 117 actual host methods and 36 independent Windows client processes on source digest `44aa16ab4a4dcbd45ea6051bc932bd17f782414196d9bd2ae3628d95ee846409`. The complete tracked/untracked source inventory was unchanged before and after the run. This is a source digest, not a new Git commit.

The same run covers committed draft history, editable restart, lost receipts, original retirement failure, attachment reselection, import cleanup, UI context changes, two libraries, disk-full refusal, and fresh original-v1 auxiliary archive maintenance. The client scenarios use framework input; they do not certify physical keyboard or IME input.

The actual main and auxiliary archive boundaries each reached 4096. Overflow was refused while preserving the original histories through restart. The actual 64 MiB aggregate archive budget also refused the next operation without changing the originals. The private disk-full volume was detached on successful completion; its disk and evidence were retained.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019\p3-combined-windows-003\result.json`. The driver is `qualify_combined_p3_003.py` beside that directory. Previous combined runs 001 and 002 remain failed evidence and are not overwritten.

This result does not accept the separate native-input run, full P3, the shared-SDK P2 retirement gate, SDK26/G04/FREEZE, or release eligibility. The manual input finding is recorded in [the native-input report](MANUAL_INPUT_WINDOWS_20261024.md).
