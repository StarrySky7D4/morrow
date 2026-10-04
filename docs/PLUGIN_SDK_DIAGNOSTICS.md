# Standalone diagnostics for an explicitly selected host

Python 3.11+ and its standard library are required.

The diagnostics source bundle provides host profile discovery and static package
preflight without a full Morrow checkout, Rust/Cargo, SDK library source or a
separately compiled package checker. It does not contain a native host executable.
The operator must select an already trusted local host and package explicitly.

## Export and inspect the source bundle

From a trusted repository checkout:

```sh
python -B tool/package_sdk_diagnostics.py create --source-root . --output ../morrow-sdk-diagnostics.zip
python -B tool/package_sdk_diagnostics.py verify-zip ../morrow-sdk-diagnostics.zip
```

The exporter refuses an existing destination. It validates the archive and
rechecks selected source observations before no-overwrite publication. Once
published, a temporary-file cleanup failure emits a warning and retains the valid
ZIP; inspect the receipt rather than treating that warning as a rollback or
permission to overwrite the destination.

Unpack the verified archive into a fresh directory. The fixed source closure is:

- `morrow_sdk_diagnostics.py`: the thin command-line entrypoint
- `sdk_profiles.py`: the original discovery/preflight implementation
- `LICENSE` and `NOTICE`: unchanged repository licensing text
- `README.md`: bundle-specific usage and boundaries
- `SDK_DIAGNOSTICS_MANIFEST.json`: the five payload files' byte inventory

The inventory reports bounded local file identities. It is not a signature,
source-authenticity proof or host qualification. Trust the tool source and the
checker itself before running them. Archive verification belongs to the repository
export tool; `verify-directory UNPACKED_DIRECTORY` checks the same flat inventory
after extraction. No compiler, Core/runtime or exporter is needed to run the
unpacked entrypoint. Keep the archive and the export receipt for byte-identity comparison.

## Use the independent entrypoint

From the unpacked directory, with the path to the trusted host explicit:

```sh
python -I -B morrow_sdk_diagnostics.py profiles --host /absolute/path/to/trusted/morrow-workbench-host
python -I -B morrow_sdk_diagnostics.py preflight /absolute/path/to/selected.mplugin --host /absolute/path/to/trusted/morrow-workbench-host
```

The entrypoint loads only its colocated `sdk_profiles.py`; it does not search for a
repository, compiler, host executable, SDK root or alternate module. The selected
host still runs native code. This is not a native-code sandbox or a grant to any
package. No automatic installation, download or host discovery is performed.

Preflight first discovers the diagnostic advertisement through the legacy-safe
`--sdk-capabilities` route. Unsupported hosts are refused before the new command
or package read. Both validated prepared and rejected receipts use the original
P01 parser and host/archive binding; this bundle has no parallel protocol decoder.

For a successfully parsed invocation, profiles/prepared return exit0, a validated
preflight rejection returns exit2, and local/protocol/process failure returns exit1
without a receipt on stdout. Argument-usage errors retain argparse exit2. A prepared
result establishes only static package preparation, not instantiation, guest
execution, dependency resolution, grants, routes or production readiness.

## Compatibility and limits

The existing SDK source distribution profile and its exact closure are unchanged.
The original `morrow_plugin.py` SDK-only and missing-runtime refusal gates for
pack/check/transform remain intact, including `check --host`. Use this explicitly
separate diagnostic entrypoint when no full repository is available.

Five seconds is the inherited subprocess/pipe deadline, followed by bounded
cleanup waits, not a whole-operation timeout. Hashing and path checks do not create
an atomic hostile-filesystem boundary. The runner does not reclaim an arbitrary
native executable's entire descendant process tree. Ordinary symlink/reparse and
nonregular preflight selections are refused, but replacement races remain.

A source bundle is neither a public binary release nor SDK/platform certification.
Linux static diagnostic execution does not qualify Windows/macOS, ordinary tokens,
protected owners, GUI, TLS/accounts or production bindings. Frozen SDK and original
guest package identities remain separate from this developer-tool artifact.

See [host discovery](PLUGIN_SDK_DISCOVERY.md) and
[selected-host preflight](PLUGIN_SDK_PREFLIGHT.md) for the inherited contracts.
