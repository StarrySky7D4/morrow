"""Qualify real directory guests and protected-owner failure/close paths on Windows.

Only fresh synthetic libraries are created. No account, firewall, VM, picker,
historical owner, or system policy is touched. All output stays on D build cache.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

from verify_editor_windows import ROOT, pin, source_snapshot
from verify_structural_repairs import source_digest

METHODS = [
    (language.lower() + '_directory_guest_preserves_protected_owner_through_reclaim_and_reopen',
     'PROTECTED_DIRECTORY_PASS language=' + language + ' ', 'actual-language-guest')
    for language in ('RUST', 'C', 'CPP')
] + [
    (name, 'PROTECTED_DIRECTORY_LIFECYCLE_PASS case=' + label + ' ', 'protected-lifecycle')
    for name, label in [
        ('missing_directory_refusal_reclaims_original_owner_and_reopens', 'missing-directory'),
        ('capture_limit_refusal_reclaims_original_owner_and_reopens', 'capture-limit'),
        ('foreign_cursor_refusal_preserves_original_selection_and_guest', 'foreign-cursor'),
        ('replayed_page_refusal_preserves_next_page_and_explicit_finish', 'page-replay-and-finish'),
        ('cancel_unread_capture_reclaims_original_native_handles_and_owner', 'cancel-unread-capture'),
        ('invalid_guest_deadline_refuses_before_admission_without_losing_selection', 'invalid-guest-deadline'),
    ]
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--wasi-sysroot', required=True, type=Path)
    args = parser.parse_args()
    output = args.output.absolute()
    cache = Path('D:/build_cache').resolve()
    target = Path(os.environ.get('CARGO_TARGET_DIR', '')).absolute()
    if (os.name != 'nt' or os.environ.get('MORROW_DISPOSABLE_WINDOWS_TEST')
            or output.exists() or not output.resolve().is_relative_to(cache)
            or not target.resolve().is_relative_to(cache)
            or Path(os.environ.get('CARGO_HOME', '')).resolve() != cache/'cargo'):
        parser.error('Fresh D output, D Cargo target/home and normal Windows host required')
    if shutil.disk_usage(cache).free < 8 * 1024**3:
        parser.error('Retain at least 8 GiB free cache space')
    output.mkdir(parents=True)
    (output/'tmp').mkdir()

    def save(name, value):
        with (output/name).open('x', encoding='utf-8') as stream:
            json.dump(value, stream, indent=2)

    before = source_snapshot()
    source = source_digest()
    save('source-before.json', before)
    save('driver.json', pin(Path(__file__)))
    environment = dict(os.environ, RUSTUP_TOOLCHAIN='1.96.0', CARGO_NET_OFFLINE='true',
        CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1', TEMP=str(output/'tmp'),
        TMP=str(output/'tmp'), TMPDIR=str(output/'tmp'),
        CAPNP=r'C:\Users\Administrator\capnp-bin\capnp.exe')
    save('environment.json', {key: environment[key] for key in
        ('CARGO_HOME', 'CARGO_TARGET_DIR', 'TEMP', 'TMP', 'RUSTUP_TOOLCHAIN', 'CAPNP')})
    processes, methods, guests = [], [], {}
    failure = None

    def run(label, command):
        command = list(map(str, command))
        save(label+'-invocation.json', dict(command=command, cwd=str(ROOT)))
        print(json.dumps(dict(state='STARTED', phase=label)), flush=True)
        start = time.monotonic()
        with (output/(label+'.stdout')).open('xb') as out, (output/(label+'.stderr')).open('xb') as err:
            result = subprocess.run(command, cwd=ROOT, env=environment, stdout=out, stderr=err)
        record = dict(phase=label, exit_code=result.returncode, seconds=round(time.monotonic()-start, 3),
            stdout=pin(output/(label+'.stdout')), stderr=pin(output/(label+'.stderr')))
        save(label+'-process.json', record)
        processes.append(record)
        if result.returncode:
            raise RuntimeError(label+' failed; original evidence retained, no automatic replay')
        return record

    try:
        toolchain = Path('C:/Users/Administrator/.rustup/toolchains/1.96.0-x86_64-pc-windows-msvc/bin')
        llvm = Path('C:/Program Files/LLVM/bin')
        run('build-c-cpp', [sys.executable, ROOT/'tool/build_directory_guest_samples.py',
            '--source-root', ROOT, '--output', output/'c-cpp-build',
            '--cargo-home', environment['CARGO_HOME'], '--sysroot', args.wasi_sysroot,
            '--cargo', toolchain/'cargo.exe', '--clang', llvm/'clang.exe',
            '--clangxx', llvm/'clang++.exe', '--wasm-ld', llvm/'wasm-ld.exe',
            '--capnp', environment['CAPNP']])
        cc = json.loads((output/'c-cpp-build/result.json').read_bytes())
        assert cc['status'] == 'PASS_NEW_C_CPP_WASM_BUILD_STATIC_ABI_ONLY'
        assert cc['original_inputs_unchanged'] and cc['tools_unchanged'] and cc['sysroot_unchanged']
        run('build-rust', [toolchain/'cargo.exe', 'build', '--manifest-path',
            ROOT/'extensions/fs-directory-request-v1/Cargo.toml', '--package',
            'morrow-fs-directory-request-guest', '--target', 'wasm32-unknown-unknown',
            '--release', '--locked', '--offline', '--target-dir', output/'rust-target'])
        paths = {'RUST': output/'rust-target/wasm32-unknown-unknown/release/morrow_fs_directory_request_guest.wasm',
            'C': output/'c-cpp-build/directory-c.wasm', 'CPP': output/'c-cpp-build/directory-cpp.wasm'}
        spec = importlib.util.spec_from_file_location('directory_build', ROOT/'tool/build_directory_guest_samples.py')
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        for language, path in paths.items():
            guests[language] = dict(pin(path), static_abi=builder.inspect_wasm(path))
            environment[f'MORROW_{language}_DIRECTORY_REQUEST_WASM'] = str(path)
            environment[f'MORROW_{language}_DIRECTORY_REQUEST_WASM_SHA256'] = guests[language]['sha256']
        save('compiled-guests.json', guests)
        run('build-host', ['cargo', 'test', '--manifest-path', ROOT/'workbench_host/Cargo.toml',
            '--locked', '--offline', '--release', '--test', 'directory_protected_sdk',
            '--no-run', '--message-format=json'])
        executables = []
        for line in (output/'build-host.stdout').read_text().splitlines():
            row = json.loads(line)
            if (row.get('reason') == 'compiler-artifact' and row.get('executable')
                    and row['target']['name'] == 'directory_protected_sdk' and row['profile']['test']):
                executables.append(Path(row['executable']))
        assert len(executables) == 1
        executable = executables[0]
        original = pin(executable)
        save('executable.json', original)
        listing = subprocess.run([str(executable), '--list'], cwd=ROOT, env=environment, capture_output=True)
        (output/'methods-list.txt').write_bytes(listing.stdout + listing.stderr)
        names = re.findall(r'^(.+): test\s*$', listing.stdout.decode(), re.M)
        assert listing.returncode == 0 and sorted(names) == sorted(name for name, _, _ in METHODS)
        for ordinal, (name, marker, scope) in enumerate(METHODS, 1):
            label = f'actual-{ordinal:02d}'
            record = run(label, [executable, '--exact', name, '--ignored', '--test-threads=1', '--nocapture'])
            text = (output/(label+'.stdout')).read_text() + (output/(label+'.stderr')).read_text()
            assert len(re.findall(r'^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out;', text, re.M)) == 1
            assert text.count(marker) == 1
            stores = re.findall(r'^PROTECTED_DIRECTORY_STORE path=(.+)$', text, re.M)
            assert len(stores) == 1 and Path(stores[0]).resolve().is_relative_to((output/'tmp').resolve())
            assert pin(executable) == original
            assert all(pin(path)['sha256'] == guests[language]['sha256'] for language, path in paths.items())
            method = dict(method=name, scope=scope, original_store=stores[0], process=record)
            methods.append(method)
            save(label+'-accepted.json', method)
            print(json.dumps(dict(state='ACTUAL_METHOD_PASSED', method=name)), flush=True)
    except Exception as error:
        failure = dict(kind=type(error).__name__, detail=str(error))
    after = source_snapshot()
    save('source-after.json', after)
    if before != after or source_digest() != source:
        failure = failure or dict(kind='SourceDrift', detail='Complete source inventory changed')
    result = dict(status='PASS_SCOPED_WINDOWS_PROTECTED_DIRECTORY_LIFECYCLE' if failure is None else 'FAILED_NOT_ACCEPTED',
        source_digest=source, source_unchanged=before == after, actual_methods=len(methods), methods=methods,
        processes=processes, compiled_guests=guests, failure=failure,
        original_protected_owner=True, original_worker_join_and_native_handle_closure=True,
        new_synthetic_libraries_only=True, account_or_policy_changes=0, vm_actions=0,
        native_picker_accepted=False, full_G04_accepted=False, full_SDK26_accepted=False,
        source_accepted=False, release_eligible=False)
    save('result.json', result)
    print(json.dumps(dict(status=result['status'], actual_methods=len(methods), failure=failure)), flush=True)
    return 0 if failure is None else 1


if __name__ == '__main__':
    raise SystemExit(main())
