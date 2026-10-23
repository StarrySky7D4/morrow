"""Qualify memory clipboard imports against the original Windows Rust host.

Uses the formal editor with synthetic clipboard content and framework keyboard
events. It does not read the system clipboard or certify native input or P3.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from verify_editor_windows import ROOT, pin, source_snapshot


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.absolute()
    cache = Path('D:/build_cache').resolve()
    target = Path(os.environ.get('CARGO_TARGET_DIR', '')).absolute()
    if (os.name != 'nt' or os.environ.get('MORROW_DISPOSABLE_WINDOWS_TEST')
            or output.exists() or not output.resolve().is_relative_to(cache)
            or not target.resolve().is_relative_to(cache)):
        parser.error('Fresh D:/build_cache output and Cargo target on the normal Windows host required')
    if shutil.disk_usage(cache).free < 8 * 1024**3:
        parser.error('Retain at least 8 GiB of build-cache free space')
    output.mkdir(parents=True)
    (output/'tmp').mkdir()
    (output/'bin').mkdir()

    def save(name, value):
        with (output/name).open('x', encoding='utf-8') as stream:
            json.dump(value, stream, indent=2)

    before = source_snapshot()
    save('source-before.json', before)
    environment = dict(os.environ, TEMP=str(output/'tmp'), TMP=str(output/'tmp'),
        TMPDIR=str(output/'tmp'), CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0',
        CARGO_BUILD_JOBS='1')
    failure = None
    phases = []
    try:
        command = ['cargo', 'build', '--manifest-path', str(ROOT/'workbench_host/Cargo.toml'),
            '--locked', '--offline', '--release', '--bin', 'morrow-workbench-host',
            '--message-format=json']
        save('host-invocation.json', dict(command=command, cwd=str(ROOT), target=str(target)))
        with (output/'host-build.jsonl').open('xb') as log, (output/'host-build.stderr').open('xb') as err:
            built = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=err)
        if built.returncode:
            raise RuntimeError('Original host build failed')
        binaries = []
        for line in (output/'host-build.jsonl').read_text(encoding='utf-8').splitlines():
            row = json.loads(line)
            if (row.get('reason') == 'compiler-artifact' and row.get('executable')
                    and row['target']['name'] == 'morrow-workbench-host'):
                binaries.append(Path(row['executable']))
        if len(binaries) != 1:
            raise RuntimeError('Original host executable is ambiguous')
        host = output/'bin/morrow-workbench-host.exe'
        shutil.copy2(binaries[0], host)
        if pin(host)['sha256'] != pin(binaries[0])['sha256']:
            raise RuntimeError('Copied original host differs')
        package = ROOT/'build/workbench-host/bundle/workbench.morrowplugin'
        originals = dict(host=pin(host), package=pin(package))
        save('original-artifacts.json', originals)
        environment.update(MORROW_WORKBENCH_HOST=str(host), MORROW_WORKBENCH_PACKAGE=str(package),
            MORROW_CLIPBOARD_TEST_ROOT=str(output/'content'))
        flutter = shutil.which('flutter.bat') or shutil.which('flutter')
        if not flutter:
            raise RuntimeError('Flutter unavailable')
        for name, test, count in [
            ('keypad', 'test/windows_editor_shortcuts_test.dart', 2),
            ('clipboard', 'test/editor_draft_clipboard_native_test.dart', 1),
            ('owner', 'test/editor_draft_owner_native_test.dart', 9),
        ]:
            command = [flutter, 'test', '--no-pub', '--reporter', 'expanded', test]
            save(name+'-invocation.json', dict(command=command, cwd=str(ROOT)))
            start = time.monotonic()
            with (output/(name+'.log')).open('xb') as log:
                run = subprocess.run(command, cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT)
            result = dict(name=name, exit_code=run.returncode, expected_methods=count,
                seconds=round(time.monotonic()-start, 3))
            save(name+'-process.json', result)
            log = (output/(name+'.log')).read_text(encoding='utf-8', errors='replace')
            if run.returncode or not re.search(r'\+'+str(count)+r':\s+All tests passed!', log):
                raise RuntimeError('Expected original test methods did not pass: '+name)
            if dict(host=pin(host), package=pin(package)) != originals:
                raise RuntimeError('Original host or package changed')
            phases.append(result)
            print(json.dumps(dict(state='PASS_SCOPED', phase=name, methods=count)), flush=True)
        receipt = json.loads((output/'content/result.json').read_bytes())
        if not (receipt['original_owner_closed'] and receipt['exact_request_recovered']
                and receipt['title_and_body_paste_accepted']
                and receipt['framework_ctrl_home_and_shift_right_accepted']
                and receipt['input_files_removed_after_host_close']
                and not receipt['business_submissions'] and not receipt['system_clipboard_read']):
            raise RuntimeError('Original clipboard close/restart receipt incomplete')
    except Exception as error:
        failure = dict(kind=type(error).__name__, detail=str(error))
    after = source_snapshot()
    save('source-after.json', after)
    if before != after:
        failure = failure or dict(kind='SourceDrift', detail='Canonical source changed')
    result = dict(status='PASS_SCOPED_FORMAL_MEMORY_CLIPBOARD_AND_OWNER' if failure is None else 'FAILED_NOT_ACCEPTED',
        source_unchanged=before == after, phases=phases, failure=failure,
        framework_input=True, native_keyboard_accepted=False, system_clipboard_read=False,
        full_P3_accepted=False, release_eligible=False)
    save('result.json', result)
    print(json.dumps(result), flush=True)
    return 0 if failure is None else 1


if __name__ == '__main__':
    raise SystemExit(main())
