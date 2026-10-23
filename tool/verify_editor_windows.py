"""Run saved-draft history or editable restart in independent Windows apps.

Uses a fresh synthetic library and framework-injected input. Does not certify
physical input, full P3, a VM runner, or release qualification. The resume
scenario separately verifies eight original clients for new and V2 cards.
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
    parser.add_argument('--scenario', choices=('history', 'resume', 'faults', 'retirement', 'diskfull', 'reselection', 'committed-history', 'import-cleanup', 'ui-context', 'workspace', 'capacity'), default='history')
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
    descriptor = json.loads((ROOT/'plugins/mid_autumn/theme.json').read_text(encoding='utf-8'))
    theme = ROOT/'build/mid-autumn-plugin/bundle'/('morrow-mid-autumn-'+descriptor['version']+'.morrowplugin')
    environment = os.environ.copy()
    selected_environment = dict(RUSTUP_TOOLCHAIN=args.rust_toolchain, CARGO_NET_OFFLINE='true',
        CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1', CMAKE_BUILD_PARALLEL_LEVEL='2',
        DART_DATA_HOME=str(output/'dart-data'), MORROW_WORKBENCH_HOST=str(host),
        MORROW_WORKBENCH_PACKAGE=str(package), MORROW_DRAFT_REVIEW_ROOT=str(output/'content'))
    environment.update(selected_environment)
    if args.scenario in ('retirement', 'diskfull'):
        python = environment.get('MORROW_CLOSE_TEST_PYTHON')
        if not python or not Path(python).is_file():
            raise ValueError('Retirement fault scenario requires its explicit Python fixture runtime')
        selected_environment['MORROW_CLOSE_TEST_PYTHON'] = python
        save('fault-runtime.json', pin(Path(python)))
    if args.scenario == 'diskfull':
        plan = environment.get('MORROW_DRAFT_VOLUME_PLAN')
        if not plan or not Path(plan).is_file():
            raise ValueError('Disk-full scenario requires a separately prepared private volume plan')
        selected_environment['MORROW_DRAFT_VOLUME_PLAN'] = plan
        save('volume-plan-pin.json', pin(Path(plan)))
    save('environment.json', selected_environment)
    # Record explicit pre-existing package reuse separately from the newly built
    # host. Hash equality cannot establish that old guest bytes came from this
    # checkout, so the result never promotes guest source qualification.
    retained = {}
    for label, path_key, hash_key, expected in (
        ('workbench', 'MORROW_RETAINED_WORKBENCH_PACKAGE', 'MORROW_RETAINED_WORKBENCH_SHA256', package),
        ('theme', 'MORROW_RETAINED_THEME_PACKAGE', 'MORROW_RETAINED_THEME_SHA256', theme),
    ):
        path, digest = environment.get(path_key), environment.get(hash_key)
        if path or digest:
            if not path or not digest or not re.fullmatch('[a-fA-F0-9]{64}', digest):
                raise ValueError('Retained '+label+' requires a path and exact SHA-256')
            original = pin(Path(path))
            if original['sha256'] != digest.lower() or pin(expected)['sha256'] != digest.lower():
                raise ValueError('Retained '+label+' package pin mismatch')
            retained[label] = original
    save('retained-packages.json', retained)
    selected_phases = {
        'history': ('seed', 'recover', 'retire', 'archive', 'history', 'live-retire'),
        'resume': tuple(f'{kind}-{step}' for kind in ('new', 'v2') for step in ('seed', 'edit', 'commit', 'verify')),
        'faults': ('business', 'prepare', 'complete', 'child', 'retire'),
        'retirement': ('fail', 'recover', 'verify'),
        'diskfull': ('full', 'verify'),
        'reselection': ('reselect', 'verify'),
        'committed-history': ('reselect', 'archive', 'verify'),
        'import-cleanup': ('imports',),
        'ui-context': ('context',),
        'workspace': ('workspace',),
        'capacity': ('capacity', 'verify'),
    }[args.scenario]
    selected_test = {
        'history': TEST,
        'resume': 'integration_test/editor_draft_resume_windows_test.dart',
        'faults': 'integration_test/editor_draft_fault_windows_test.dart',
        'retirement': 'integration_test/editor_draft_retirement_windows_test.dart',
        'diskfull': 'integration_test/editor_draft_disk_full_windows_test.dart',
        'reselection': 'integration_test/editor_draft_reselection_windows_test.dart',
        'committed-history': 'integration_test/editor_draft_reselection_windows_test.dart',
        'import-cleanup': 'integration_test/editor_draft_fault_windows_test.dart',
        'ui-context': 'integration_test/editor_draft_fault_windows_test.dart',
        'workspace': 'integration_test/editor_draft_workspace_windows_test.dart',
        'capacity': 'integration_test/editor_draft_capacity_windows_test.dart',
    }[args.scenario]
    phases = []
    failure = None
    try:
        for phase in selected_phases:
            environment['MORROW_DRAFT_REVIEW_PHASE'] = phase
            content_name = ('content-'+phase if args.scenario == 'faults' else
                ('content-'+phase.split('-')[0]) if args.scenario == 'resume' else
                ('content-live' if phase == 'live-retire' else 'content'))
            content = output/content_name
            environment['MORROW_DRAFT_REVIEW_ROOT'] = str(content)
            command = [flutter, 'test', selected_test, '-d', 'windows', '--no-pub', '--reporter', 'expanded']
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
            result_name = 'recover-review.json' if args.scenario == 'history' and phase == 'recover' else phase+'.json'
            result = json.loads((content/result_name).read_bytes())
            if (not window['visible'] or not result['complete'] or window['pid'] != result['pid']
                    or 'flutter_tester' in window['executable'].lower()
                    or window['physical_input_accepted'] is not False):
                raise RuntimeError('Original window/client/result identity mismatch: ' + phase)
            executable = Path(window['executable'])
            assets = executable.parent/'data/flutter_assets'
            installed_kernel = assets/'kernel_blob.bin'
            assembled_kernel = ROOT/'build/flutter_assets/kernel_blob.bin'
            if (not assets.is_dir() or assets.is_symlink() or assets.is_junction()
                    or not assets.resolve().is_relative_to((ROOT/'build/windows').resolve())
                    or pin(installed_kernel)['sha256'] != pin(assembled_kernel)['sha256']):
                raise RuntimeError('Installed Windows assets do not match the assembled build: ' + phase)
            client_artifacts = dict(executable=pin(executable), kernel=pin(installed_kernel),
                engine=pin(executable.parent/'flutter_windows.dll'))
            artifacts = dict(host=pin(host), package=pin(package), theme=pin(theme))
            if not phases:
                originals = artifacts
                save('original-artifacts.json', originals)
            elif artifacts != originals:
                raise RuntimeError('Original host/package changed between client processes')
            phases.append(dict(row, window=window, result=result, client_artifacts=client_artifacts))
            print(json.dumps(dict(phase=phase, status='ONE_NATIVE_WINDOWS_CLIENT_PASS',
                pid=window['pid'])), flush=True)
        if len({phase['window']['pid'] for phase in phases}) != len(selected_phases):
            raise RuntimeError('Expected distinct independent client processes')
    except Exception as error:
        failure = dict(kind=type(error).__name__, detail=str(error))
    after = source_snapshot()
    save('source-after.json', after)
    if after != before:
        failure = failure or dict(kind='SourceDrift', detail='Source inventory or bytes changed during run')
    statuses = dict(history='NATIVE_WINDOWS_FORMAL_DRAFT_LIFECYCLE_SCOPED_PASS',
        resume='NATIVE_WINDOWS_FORMAL_EDITABLE_RESTART_SCOPED_PASS',
        faults='NATIVE_WINDOWS_FORMAL_RECEIPT_LOSS_SCOPED_PASS',
        retirement='NATIVE_WINDOWS_FORMAL_RETIREMENT_FAILURE_RESTART_SCOPED_PASS',
        diskfull='NATIVE_WINDOWS_FORMAL_DISK_FULL_RECOVERY_RESTART_SCOPED_PASS',
        reselection='NATIVE_WINDOWS_FORMAL_ATTACHMENT_RESELECTION_RESTART_SCOPED_PASS',
        **{'committed-history': 'NATIVE_WINDOWS_FORMAL_COMMITTED_PARENT_HISTORY_RESTART_SCOPED_PASS',
           'import-cleanup': 'NATIVE_WINDOWS_FORMAL_IMPORT_CLEANUP_LOST_REPLY_SCOPED_PASS',
           'ui-context': 'NATIVE_WINDOWS_FORMAL_LOCALE_FONT_PENDING_COMMIT_SCOPED_PASS',
           'workspace': 'NATIVE_WINDOWS_FORMAL_TWO_WORKSPACE_HANDOFF_SCOPED_PASS',
           'capacity': 'NATIVE_WINDOWS_FORMAL_CAPACITY_HISTORY_RESTART_SCOPED_PASS'})
    save('result.json', dict(status=statuses[args.scenario]
        if failure is None else 'FAILED_NOT_ACCEPTED', phases=phases, failure=failure,
        source_unchanged=before == after, physical_input_accepted=False, rendered_pixels_verified=False,
        retained_packages=retained, guest_source_accepted=False,
        editable_restart_accepted=args.scenario == 'resume' and failure is None,
        formal_receipt_loss_accepted=args.scenario == 'faults' and failure is None,
        retirement_failure_restart_accepted=args.scenario == 'retirement' and failure is None,
        disk_full_recovery_restart_accepted=args.scenario == 'diskfull' and failure is None,
        attachment_reselection_restart_accepted=args.scenario == 'reselection' and failure is None,
        committed_parent_history_restart_accepted=args.scenario == 'committed-history' and failure is None,
        import_cleanup_lost_reply_accepted=args.scenario == 'import-cleanup' and failure is None,
        locale_font_pending_commit_accepted=args.scenario == 'ui-context' and failure is None,
        two_workspace_handoff_accepted=args.scenario == 'workspace' and failure is None,
        formal_capacity_history_restart_accepted=args.scenario == 'capacity' and failure is None,
        full_P3_accepted=False, release_eligible=False))
    if failure:
        print(json.dumps(failure), flush=True)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
