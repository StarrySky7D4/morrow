# X01: standalone selected-host diagnostics source distribution

2026-10-04. Baseline `8ba1aa76823b18d6fcbc578b3f9b380dc017e92d`, tree
`9a00e815f32f2c19f947b52263661ccff00422c1`. All 51,375 baseline source paths
were verified. The existing worktree and native host artifacts were reused; this
stage creates no new Rust target, native executable or whole-repository copy.

## Developer-visible result

A small source ZIP now provides `profiles --host HOST` and
`preflight PACKAGE --host HOST` outside a full repository checkout. The new thin
entrypoint explicitly loads its adjacent, unchanged `sdk_profiles.py`, including
under Python isolated mode. It delegates to the one existing `query` / `preflight`
implementation; it adds no protocol decoder, host implementation or authority.

The fixed six-file ZIP contains two Python sources, the original LICENSE/NOTICE,
bundle-specific README and `SDK_DIAGNOSTICS_MANIFEST.json`. It contains no host,
guest package, SDK library, Core/runtime, compiler, exporter or test suite. The
repository-side exporter/verifier checks fixed closure, bounded size, paths,
regular-file types, CRC and hashes without executing archive code. Its manifest
is local byte identity, not a signature or proof that self-rehashed code is safe.

Source inputs are observed before and after packaging. Publication refuses an
existing output, including a racing new destination. All validation happens before
no-overwrite publication. After publication, temporary cleanup failure warns and
retains the published archive; it cannot blindly delete a destination that another
actor may have replaced. This is not hostile-filesystem containment.

The old SDK source-v1 exporter/closure, project-tool SDK-only and missing-runtime
pack/check/transform refusals, and P01 protocol validator remain byte-identical.
The separate entrypoint is the explicit way to diagnose an installed trusted host
without those repository tools. No default mode silently changes its meaning.

See [the standalone guide](../../docs/PLUGIN_SDK_DIAGNOSTICS.md).

## Actual validation

| Evidence | Observed result | Scope |
|---|---|---|
| Final focused Python invocation | 176 passed, no errors/failures or ResourceWarnings | One deduplicated 11-module invocation: original P01 150 plus X01 26 |
| Real exported bundle preflight matrix | 27 expected outcomes | 20 statically prepared, seven validated structured rejections |
| Real exported profile query | Passed | Original descriptor parser, selected host digest bound |
| Actual legacy-host boundary | Direct and audit-observed calls refused safely | Each only asks for `--sdk-capabilities`; no new flag/package read |
| ZIP and extracted-directory checks | Passed before/after standalone use | Fixed six files, unchanged byte identities, no cache artifacts |

The 176 method result is an actual single invocation with warnings enabled, not a
sum of historical runs. Earlier 24/26 new-tool, 98 SDK-related and 15 legacy-export
runs are retained as intermediate/regression evidence and are not extra passes.
The command matrix overlaps P01 packages and is not another SDK method total.

The actual artifact was created, verified and unpacked to a fresh directory.
Its CLI ran there using absolute Python 3.12, `-I -B`, empty PATH and unset
PYTHONPATH/PYTHONHOME. Each case had fresh external CWD/HOME/TMP directories.
No repository validator was imported by the executing consumer. Independent source
loading, not merely `--help`, was exercised by profiles, prepared and rejected
results. Ordinary invocation without `-B` is separately tested to leave no bytecode.

The real matrix covers 13 unchanged guest originals, six unchanged IO/service
transport originals and one trap-entry fixture as static successes. Seven original
P01 negative fixtures cover bad archive/module, forbidden start/import, wrong entry,
unknown required feature and oversized embedded module. All seven retain exit2
and their validated phase/error; success does not mean instantiation or execution.
Source files, selected archives, both native executables and the extracted bundle
were unchanged after the actual run. Fresh CWD/HOME/TMP remained empty.

Legacy refusal was tested directly and through a Python audit-hook launcher that
records subprocess argv without replacing the validator. That observation confirms
only the original capabilities command was sent. It is not syscall tracing or an
OS-wide no-effect proof. The missing package was never needed after unsupported
capability discovery.

Exact artifact: 23,437 bytes; SHA256
`decff0a685926c6d65d7e55a97b1d5b7539ce9bf728b071b4a086b77936d94a3`.
The reused Linux Debug P01 host SHA256 is
`655f8881c1392749d366a0c0fdb90617ed7707f27f8f28733470a4819d778fa8`;
the legacy D01 host is
`65460b70194a25c332ec502de5d369886166316fd838582ea9a15ac45686122d`.
No host was recompiled in X01. Their existing Rust1.95/Cap'n Proto1.4 provenance
remains the earlier phase's toolchain evidence, not a new compiler qualification.

## Review-driven corrections

- ZIP validation now rejects raw/original entry names that Python would truncate
  at NUL, before checking the normalized fixed closure
- Export cleanup cannot delete a published destination after temporary unlink
  failure. Regression covers that warning and destination preservation
- Source/output `..` aliases are canonicalized only after each original path
  component has passed link/reparse rejection. Output-parent/source aliases are
  covered within the same existing test method; final export and all 27 actual
  cases were rerun after this correction, producing the identical source ZIP
- Independent runtime tests were tightened from copied source / extracted help
  checks to actual export, verification, extraction and validator-exercising calls
- The adjacent consumer is loaded without repository/PYTHONPATH fallback and
  without creating bytecode, preserving the exact bundle closure

Independent source/contract review is hash-bound to the delivered implementation;
it is not an additional build or executed test suite. Intermediate logs remain
retained rather than relabeled as the final source's complete verification.
An evidence helper initially treated the expected exit1 from `git diff --no-index`
on newly added files as a failure despite no whitespace diagnostics. Its assertion
was corrected and the failed helper log retained; no source, test or ZIP changed.

## Boundaries still open

The operator must trust the native host, tool source and local file selections.
P01's five-second subprocess/pipe deadline plus bounded cleanup is not a whole-call
or whole-operation deadline; host hashing has no independent total-time guarantee.
The runner does not reclaim an arbitrary native executable's entire process tree.
Before/after file observations do not prevent adversarial replace-and-restore or
all ancestor races. A manifest with self-rehashed malicious code is not provenance.

Static `prepared` does not establish guest execution, runtime budget fit, grants,
dependency resolution, routes, installation or production readiness. X01 provides
no new Windows/macOS, ordinary-token, protected owner/key, GUI, public TLS/account
or complete SDK qualification. SDK327/frozen57, original Core/Runner/Rust sources,
schemas, pins and locks remain unchanged. Historical platform totals retain their
own scope; the complete target SDK remains OPEN.
