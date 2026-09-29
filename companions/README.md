# Companion source snapshot

`morrow-codex/` is a byte-preserving source snapshot of the separate local
Morrow Codex qualification repository, captured on 2026-09-29. The original
repository has no remote and is retained unchanged. This directory is an archive
for review, not an installed plugin, Flutter integration, or a frozen SDK.

The snapshot retains its own directory structure, source manifests, Cargo locks,
build-forks, fixed upstream inputs, licenses, qualification code and receipts.
Upstream code remains under its original licenses; Morrow's root license does not
relicense these dependencies. Generated executables, databases, archives, caches,
and evidence files larger than 20 MB are in the paired Drive backup instead.
The `out/**/build-forks` and `out/**/git-directory-sources` inputs are deliberately
included: despite their path names, they are build inputs, not disposable outputs.

To continue in the original two-repository layout, copy this snapshot to a
separate `morrow-codex` directory alongside `morrow`, or restore the paired Drive
archive. Original receipts may contain machine-specific absolute paths. Restoring
elsewhere requires an explicit path adaptation and fresh validation; old receipts
must not be rewritten to claim a run on the new machine. Preserve symbolic links
when restoring upstream sources. Cargo dependencies omitted from the backup can
be downloaded using the retained locks and build scripts; an offline build is
not guaranteed.

The archived Git blobs preserve source bytes. Some inherited upstream and kit
`.gitattributes` rules normalize line endings on checkout; for byte-bound evidence
use the paired ZIP backup or `git archive`, then verify the original manifests.
Do not restamp evidence hashes to accommodate checkout normalization. Windows Git
may require `core.longpaths=true` for the deep upstream test fixture paths.

The snapshot's historical README describes its initial P-00/P-01 state. For the
current qualified scope and remaining gaps, see
[`delivery-2026-09-29.md`](../reports/codex-morrow-v1.1/delivery-2026-09-29.md).
