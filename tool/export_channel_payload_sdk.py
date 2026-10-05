"""Export an independent experimental source bundle from the trusted repository.

No compiler, Core executable, runtime, guest, service or network is invoked.
"""
import argparse
import json
import os
from pathlib import Path
import tempfile
import zipfile

import package_plugin_sdk as original
import plugin_sdk_lock as sdk_lock
import verify_channel_payload_sdk as bundle

README = b"""# Morrow experimental channel payload SDK

This source-only bundle contains the unchanged original transport SDK, typed WebSocket
message and SSE event codecs, C11/C++17 headers and three-language guest examples.
Both payload libraries are experimental. No host, runtime, Core, precompiled library,
guest, package, credential, grant or third-party toolchain is included.

Python 3.11+ verifies or extracts the exact bounded inventory:

    python -B tool/verify_channel_payload_sdk.py verify-directory .
    python -B tool/verify_channel_payload_sdk.py verify-zip ../channel-sdk.zip
    python -B tool/verify_channel_payload_sdk.py extract-zip ../channel-sdk.zip ../fresh-sdk

Extraction and repository export refuse any existing destination. Keep this source
directory unchanged and place build outputs outside it. Preserve sdk/ and extensions/
relative paths when relocating the bundle. The original SDK-only project CLI, host
package builder and repository-only guest preparation tools are not included or
claimed compatible with this independent profile.

Use an already provisioned trusted Rust/Cargo toolchain, Cap'n Proto compiler and
offline dependency cache. For each extension, ordinary trusted native builds are:

    cargo build --release --offline --locked --manifest-path extensions/ws-message-v1/Cargo.toml --package morrow-ws-message-v1 --target-dir ../ws-native
    cargo build --release --offline --locked --manifest-path extensions/sse-event-v1/Cargo.toml --package morrow-sse-event-v1 --target-dir ../sse-native

Rust guest examples use the same workspace locks and require wasm32-unknown-unknown:

    cargo build --release --offline --locked --manifest-path extensions/ws-message-v1/Cargo.toml --package morrow-ws-message-guest --target wasm32-unknown-unknown --target-dir ../ws-wasm
    cargo build --release --offline --locked --manifest-path extensions/sse-event-v1/Cargo.toml --package morrow-sse-event-guest --target wasm32-unknown-unknown --target-dir ../sse-wasm

C/C++ guests additionally require a trusted Clang WASI compiler/sysroot. Build each
extension for wasm32-unknown-unknown with --features c-transport (one static archive includes
the unchanged SDK C transport). Compile the five sdk/c/src files named
morrow_plugin_sdk.c, morrow_plugin_wasm.c, morrow_plugin_wasm_libc.c,
morrow_plugin_task.c and morrow_channel_v1.c as C11 objects. Use --target=wasm32-wasip1,
--sysroot=PATH, -O2 -Wall -Wextra -Werror and the extension's include/ with sdk/c/include.
Link guests/c/plugin.c, those objects and finally the extension archive using
-nostdlib -Wl,--no-entry -Wl,--export=morrow_run -Wl,-z,stack-size=1048576
-Wl,--max-memory=16777216 -Wl,--strip-all and the WASI libc search path with -lc.

For C++17, also compile sdk/cpp/src/morrow_plugin_wasm_runtime.cpp, add sdk/cpp/include,
-nostdinc++, the sysroot's include/wasm32-wasip1/noeh/c++/v1 system include,
-fno-exceptions -fno-rtti, and compile guests/cpp/plugin.cpp with
-Dmorrow_run=mp_guest_run. Link that guest, the runtime object, the C objects, the
extension archive and WASI noeh libc++/libc++abi/libc, additionally exporting
__wasm_call_ctors. Original host import modules remain morrow_v1, morrow_task_v1 and
morrow_channel_v1; a host-controlled package/approval is still required to execute.
Toolchain, WASI libc and C++ linker requirements are external build prerequisites.

Inventory verification proves bounded byte consistency and the exact v1 Cargo input
graph, not a signature, malicious-source safety, compiler expansion or semantic
compatibility. A build is normal trusted code execution, not a sandbox. The export
gate compares SDK contracts with the repository's canonical Core schemas and each
payload schema with its original native schema; these authorities are not bundled.
Independent verification cannot recreate host agreement or production approval.

WebSocket and SSE envelopes only interpret payload bytes. Codec names are not new
package permissions or host imports. Receive/ACK/Send require host-controlled source
bindings, current permissions, budgets and lifecycle. EOF, id, retry and [DONE] do not
authorize reconnect, replay or business success. Unknown never automatically replays.
Full SDK freeze, production owner/GUI and other-platform qualification remain open.
"""
NOTICE = b"""# Source inventory boundary

The original SDK library and licenses are copied unchanged. Newly typed payload
extensions remain separate experimental libraries licensed AGPL-3.0-only. This new
source profile does not widen the old SDK327/frozen57 contract or old distribution.
SHA256 is an unsigned inventory, not third-party origin authentication. No credential,
database, account, production approval, owner capability or execution evidence is
included. Build and runtime qualification require separate pinned receipts.
"""


def source_authority(root):
    authority = original.validate_source_authority(root)
    for extension, record in bundle.CONTRACTS.items():
        name = "ws_message.capnp" if extension == "ws-message-v1" else "sse_event.capnp"
        native = bundle.read_file(root, "network_node_stream_001/schemas/" + name)
        copied = bundle.read_file(root, "extensions/" + extension + "/contracts/" + name)
        if native != copied or bundle.sha(native) != record["schema_sha256"]:
            raise bundle.BundleError("native payload contract differs before export")
    return authority


def snapshot(root):
    root = bundle.checked_root(root)
    selected_sdk = {"sdk/" + name for name in sdk_lock.inventory(root / "sdk")}
    if selected_sdk != bundle.SDK_NAMES:
        raise bundle.BundleError("original SDK source inventory differs from exact v1 closure")
    files = {name: bundle.read_file(root, name) for name in sorted(bundle.NAMES - {"README.md", "NOTICE.md"})}
    files["README.md"], files["NOTICE.md"] = README, NOTICE
    manifest = bundle.build_manifest(files)
    bundle.verify_payload(files, manifest)
    return files, manifest


def archive_files(files, manifest, output):
    output = Path(os.path.abspath(output))
    bundle.checked_root(output.parent)
    if os.path.lexists(output):
        raise bundle.BundleError("output already exists")
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix="channel-sdk-", suffix=".tmp", dir=output.parent, delete=False) as stream:
            temporary = Path(stream.name)
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name, data in sorted({**files, bundle.MANIFEST: manifest}.items()):
                info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = 0o100644 << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data)
        bundle.verify_zip(temporary)
        return temporary
    except BaseException:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
        raise


def create_archive(root, output):
    root, output = bundle.checked_root(root), Path(os.path.abspath(output))
    authority = source_authority(root)
    files, manifest = snapshot(root)
    temporary = archive_files(files, manifest, output)
    try:
        again_files, again_manifest = snapshot(root)
        if (again_files != files or again_manifest != manifest or source_authority(root) != authority):
            raise bundle.BundleError("source or canonical authority changed during export")
        # Atomic publication refuses existing files/directories/links; no overwrite.
        os.link(temporary, output)
        verification = bundle.verify_zip(output)
        with output.open("rb") as stream:
            data = stream.read(bundle.MAX_ARCHIVE_BYTES + 1)
        if len(data) > bundle.MAX_ARCHIVE_BYTES:
            raise bundle.BundleError("published archive exceeds byte budget")
        return {**verification, "bytes": len(data), "sha256": bundle.sha(data),
                "source_authority": authority, "repository_export_only": True}
    finally:
        temporary.unlink(missing_ok=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--root", type=Path, default=Path(__file__).parent.parent)
    args = parser.parse_args(argv)
    try:
        result = create_archive(args.root, args.output)
    except (bundle.BundleError, original.DistributionError, sdk_lock.SdkLockError, OSError, ValueError) as error:
        parser.exit(2, "Source bundle export refused: " + str(error) + "\n")
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
