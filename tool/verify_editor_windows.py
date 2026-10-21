"""Run saved-draft lifecycle in four real Windows application processes.

Uses a fresh synthetic library and framework-injected input. Does not certify
physical input, editable restart, full P3, a VM runner, or release qualification.
"""
from __future__ import annotations
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
TEST = 'integration_test/editor_draft_lifecycle_windows_test.dart'


def pin(path: Path) -> dict:
    raw = path.read_bytes()
    return {'path': str(path), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}


def source_snapshot() -> dict:
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=ROOT)
    names = set(git('ls-files', '-z').split(b'\0'))
    names.update(git('ls-files', '--others', '--exclude-standard', '-z').split(b'\0'))
    names.discard(b'')
    files = {}
    for raw in sorted(names):
        name = os.fsdecode(raw)
        path = ROOT/name
        if path.is_symlink() or path.is_junction() or not path.resolve().is_relative_to(ROOT):
            raise ValueError('Source link or out-of-root input: ' + name)
        files[name] = pin(path)['sha256'] if path.is_file() else 'MISSING'
    return {'head': git('rev-parse', 'HEAD').decode().strip(), 'files': files}


def check_windows_build_cache() -> None:
    cache = ROOT/'build/windows/x64/CMakeCache.txt'
    if not cache.exists():
        return
    values = {}
    for line in cache.read_text(encoding='utf-8').splitlines():
        if '=' in line and ':' in line.split('=', 1)[0]:
            key, value = line.split('=', 1)
            values[key.split(':', 1)[0]] = value
    if Path(values.get('CMAKE_HOME_DIRECTORY', '')).resolve() != ROOT/'windows':
        raise ValueError('Existing CMake cache belongs to another source tree')
    prefix = values.get('CMAKE_INSTALL_PREFIX', '')
    # This repository installs beside its own generated executable. An absolute
    # caller-selected destination needs a separate review before reusing a cache.
    if prefix != '$<TARGET_FILE_DIR:morrow_studio>':
        raise ValueError('Existing Windows install destination is not the repository default')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New evidence directory; never reused')
    parser.add_argument('--rust-toolchain', default='1.96.0')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Requires actual Windows application execution')
    flutter = shutil.which('flutter.bat') or shutil.which('flutter')
    if not flutter:
        parser.error('Flutter must be installed on PATH')
    output = args.output.absolute()
    if output.exists():
        parser.error('Output already exists; preserve the original run and select a new directory')
    if output.resolve().is_relative_to(ROOT) and not output.resolve().is_relative_to(ROOT/'build'):
        parser.error('In-repository evidence must be under ignored build/')
    for parent in output.parents:
        if parent.exists() and (parent.is_symlink() or parent.is_junction()):
            parser.error('Evidence path cannot traverse a link or junction')
    check_windows_build_cache()
    output.mkdir(parents=True)

    def save(name, value):
        with (output/name).open('x', encoding='utf-8', newline='\n') as stream:
            json.dump(value, stream, ensure_ascii=False, indent=2)
            stream.write('\n')

    before = source_snapshot()
    save('source-before.json', before)
    host = ROOT/'build/workbench-host/release/morrow-workbench-host.exe'
    package = ROOT/'build/workbench-host/bundle/workbench.morrowplugin'
    environment = os.environ.copy()
    selected_environment = dict(RUSTUP_TOOLCHAIN=args.rust_toolchain, CARGO_NET_OFFLINE='true',
        CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1', CMAKE_BUILD_PARALLEL_LEVEL='2',
        DART_DATA_HOME=str(output/'dart-data'), MORROW_WORKBENCH_HOST=str(host),
        MORROW_WORKBENCH_PACKAGE=str(package), MORROW_DRAFT_REVIEW_ROOT=str(output/'content'))
    environment.update(selected_environment)
    save('environment.json', selected_environment)
    phases = []
    failure = None
    try:
        for phase in ('seed', 'recover', 'retire', 'live-retire'):
            environment['MORROW_DRAFT_REVIEW_PHASE'] = phase
            content = output/('content-live' if phase == 'live-retire' else 'content')
            environment['MORROW_DRAFT_REVIEW_ROOT'] = str(content)
            command = [flutter, 'test', TEST, '-d', 'windows', '--no-pub', '--reporter', 'expanded']
            save(phase+'-invocation.json', dict(command=command, phase=phase,
                cwd=str(ROOT), started_utc=dt.datetime.now(dt.timezone.utc).isoformat()))
            started = time.monotonic()
            with (output/(phase+'.log')).open('xb') as log:
                process = subprocess.run(command, cwd=ROOT, env=environment,
                    stdout=log, stderr=subprocess.STDOUT)
            row = dict(phase=phase, exit_code=process.returncode,
                seconds=round(time.monotonic()-started, 3))
            save(phase+'-process.json', row)
            log = (output/(phase+'.log')).read_text(encoding='utf-8', errors='replace')
            if process.returncode or not re.search(r'\+1:\s+All tests passed!', log):
                raise RuntimeError('Expected exactly one successful native client test: ' + phase)
            window = json.loads((content/f'window-{phase}.json').read_bytes())
            result_name = 'recover-review.json' if phase == 'recover' else phase+'.json'
            result = json.loads((content/result_name).read_bytes())
            if (not window['visible'] or not result['complete'] or window['pid'] != result['pid']
                    or 'flutter_tester' in window['executable'].lower()
                    or window['physical_input_accepted'] is not False):
                raise RuntimeError('Original window/client/result identity mismatch: ' + phase)
            artifacts = dict(host=pin(host), package=pin(package))
            if not phases:
                originals = artifacts
                save('original-artifacts.json', originals)
            elif artifacts != originals:
                raise RuntimeError('Original host/package changed between client processes')
            phases.append(dict(row, window=window, result=result))
            print(json.dumps(dict(phase=phase, status='ONE_NATIVE_WINDOWS_CLIENT_PASS',
                pid=window['pid'])), flush=True)
        if len({phase['window']['pid'] for phase in phases}) != 4:
            raise RuntimeError('Expected four independent client processes')
    except Exception as error:
        failure = dict(kind=type(error).__name__, detail=str(error))
    after = source_snapshot()
    save('source-after.json', after)
    if after != before:
        failure = failure or dict(kind='SourceDrift', detail='Source inventory or bytes changed during run')
    save('result.json', dict(status='NATIVE_WINDOWS_FORMAL_DRAFT_LIFECYCLE_SCOPED_PASS'
        if failure is None else 'FAILED_NOT_ACCEPTED', phases=phases, failure=failure,
        source_unchanged=before == after, physical_input_accepted=False, rendered_pixels_verified=False,
        editable_restart_accepted=False, full_P3_accepted=False, release_eligible=False))
    if failure:
        print(json.dumps(failure), flush=True)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
