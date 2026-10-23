"""Build and test the derived Windows Cap'n Proto codec/state machines only.

This never starts the sandbox runner, provisioning service, account setup or VM.
Synthetic native-close facts do not prove actual native closure or SDK release.
Use a fresh D:\\build_cache evidence directory and a stable source checkout.
"""
from __future__ import annotations
import argparse
import datetime as dt
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
PREFIXES = (
    'runner_wire::tests::',
    'elevated::ipc_framed::tests::',
    'elevated::ipc_framed::legacy_compatibility_tests::',
    'elevated::ipc_framed::checked_protocol::tests::',
    'elevated::native_close_protocol::',
    'elevated::native_close_runtime::tests::',
)
RUNNER_TEST = 'checked_input_capnp_reader_rejects_json_and_partial_boundaries_without_native_resources'

def pin(path):
    data = Path(path).read_bytes()
    return dict(path=str(path), bytes=len(data), sha256=hashlib.sha256(data).hexdigest())

def snapshot():
    paths = set()
    for flags in (['--cached'], ['--others', '--exclude-standard']):
        paths.update(subprocess.check_output(['git', 'ls-files', '-z', *flags], cwd=ROOT).split(b'\0'))
    paths.discard(b'')
    result = {}
    for item in sorted(paths):
        name = os.fsdecode(item); path = ROOT / name
        if path.is_symlink() or path.is_junction() or not path.resolve().is_relative_to(ROOT):
            raise ValueError('Source input link/outside source root: ' + name)
        result[name] = pin(path)['sha256'] if path.is_file() else 'MISSING'
    return result

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    out = a.output.absolute()
    if os.name != 'nt' or out.exists() or not out.is_relative_to(Path('D:/build_cache')):
        p.error('Requires Windows and a fresh absolute directory inside D:/build_cache')
    if any(x.exists() and (x.is_symlink() or x.is_junction()) for x in out.parents):
        p.error('Evidence directory cannot traverse links')
    target = Path(os.environ.get('CARGO_TARGET_DIR', ''))
    temp = Path(os.environ.get('TEMP', ''))
    if not target.is_absolute() or not target.resolve().is_relative_to(Path('D:/build_cache')) or not temp.resolve().is_relative_to(Path('D:/build_cache')):
        p.error('Cargo and temporary files must use D:/build_cache')
    if os.environ.get('MORROW_DISPOSABLE_WINDOWS_TEST'):
        p.error('Refusing a host environment enabling disposable sandbox effects')
    out.mkdir(parents=True)
    def save(name, value):
        with (out/name).open('x', encoding='utf-8', newline='\n') as f:
            json.dump(value, f, ensure_ascii=False, indent=2); f.write('\n')
    before = snapshot(); save('source-before.json', before)
    save('environment.json', {name: os.environ.get(name) for name in ('CARGO_TARGET_DIR','CARGO_HOME','TEMP','TMP','RUSTUP_TOOLCHAIN','CAPNP','CARGO_NET_OFFLINE','CARGO_INCREMENTAL','CARGO_BUILD_JOBS')})
    runs = []
    def run(name, args, timeout):
        started = time.monotonic(); code = None
        try:
            with (out/(name+'.stdout')).open('xb') as stdout, (out/(name+'.stderr')).open('xb') as stderr:
                code = subprocess.run([str(x) for x in args], cwd=ROOT, stdout=stdout, stderr=stderr, timeout=timeout).returncode
        finally:
            record = dict(name=name, command=[str(x) for x in args], exit_code=code, seconds=round(time.monotonic()-started,3))
            runs.append(record); save(name+'.result.json',record)
        if code:
            raise RuntimeError(name+' failed: '+str(code))
        return (out/(name+'.stdout')).read_bytes()
    status = 'FAILED_NOT_ACCEPTED'; failure = None; executable = None; selected = []
    try:
        sandbox = ROOT/'companions/morrow-codex/derived/c28-cleanup-com-002/codex-rs/windows-sandbox-rs'
        protocol_sources = ['src/runner_wire.rs','src/framed_io.rs','src/elevated/ipc_framed.rs',
            'src/elevated/native_close_protocol.rs','src/elevated/native_close_runtime.rs',
            'src/elevated/native_close_codec.rs','src/bin/command_runner/win/checked_lifecycle.rs']
        audit = {}
        for name in protocol_sources:
            source = (sandbox/name).read_text(encoding='utf-8')
            production = source if name.endswith('checked_lifecycle.rs') else source.split('#[cfg(test)]',1)[0]
            code = '\n'.join(line for line in production.splitlines() if not line.lstrip().startswith('//'))
            audit[name] = dict(sha256=pin(sandbox/name)['sha256'], production_json_calls=bool(re.search(r'\bserde_json\s*(?:::|;)',code)))
        save('codec-source-audit.json',audit)
        if any(row['production_json_calls'] for row in audit.values()):
            raise RuntimeError('JSON remains in the selected production transport or authentication path')
        cargo = shutil.which('cargo')
        if not cargo: raise RuntimeError('Cargo unavailable')
        build = run('compile', [cargo,'test','--offline','--locked','--manifest-path',sandbox/'Cargo.toml',
            '-p','codex-windows-sandbox','--lib','--bin','codex-command-runner','--no-run','--message-format=json'],900)
        candidates = []; runners = []
        for line in build.splitlines():
            event = json.loads(line)
            if event.get('reason') == 'compiler-artifact' and event.get('executable') and event['target']['name'] == 'codex_windows_sandbox':
                candidates.append(Path(event['executable']))
            if event.get('reason') == 'compiler-artifact' and event.get('executable') and event['target']['name'] == 'codex-command-runner':
                runners.append(Path(event['executable']))
        if len(candidates)!=1: raise RuntimeError('Expected one original library test artifact')
        original = pin(candidates[0]); executable = out/'codex-windows-sandbox-ipc-tests.exe'
        shutil.copyfile(candidates[0],executable)
        if pin(executable)['sha256'] != original['sha256']: raise RuntimeError('Test artifact copy mismatch')
        save('artifact.json',dict(original=original,retained=pin(executable)))
        names = run('inventory',[executable,'--list','--format','terse'],30).decode().splitlines()
        for prefix in PREFIXES:
            subset = [line[:-6] for line in names if line.startswith(prefix) and line.endswith(': test')]
            if not subset: raise RuntimeError('Missing test family '+prefix)
            selected.extend(subset)
            # A filtered family cannot execute any legacy setup/sandbox test.
            run('tests-'+str(len(runs)),[executable,prefix,'--test-threads=1','--nocapture'],180)
        if len(runners)!=1: raise RuntimeError('Expected one original Runner test artifact')
        original_runner=pin(runners[0]); runner=out/'codex-runner-ipc-tests.exe'
        shutil.copyfile(runners[0],runner)
        if pin(runner)['sha256']!=original_runner['sha256']: raise RuntimeError('Runner test artifact copy mismatch')
        save('runner-artifact.json',dict(original=original_runner,retained=pin(runner)))
        runner_names=run('runner-inventory',[runner,'--list','--format','terse'],30).decode().splitlines()
        matches=[line[:-6] for line in runner_names if line.endswith('::'+RUNNER_TEST+': test')]
        if len(matches)!=1: raise RuntimeError('Expected the single native-resource-free Runner input test')
        selected.extend(matches)
        run('runner-input',[runner,matches[0],'--exact','--test-threads=1','--nocapture'],30)
        status = 'SCOPED_WINDOWS_CAPNP_CODEC_AND_STATE_MACHINE_PASS'
    except Exception as e:
        failure = type(e).__name__+': '+str(e)
    finally:
        after=snapshot(); save('source-after.json',after)
        if before!=after:
            status='SOURCE_CHANGED_NOT_ACCEPTED'
        digest=hashlib.sha256()
        for name,value in sorted(before.items(),key=lambda item: os.fsencode(item[0])):
            raw=os.fsencode(name); digest.update(len(raw).to_bytes(4,'little')); digest.update(raw)
            digest.update(bytes.fromhex(value) if value!='MISSING' else b'MISSING')
        save('result.json',dict(status=status,utc=dt.datetime.now(dt.UTC).isoformat(),source_unchanged=before==after,
            source_digest=digest.hexdigest(),manifest_digest=hashlib.sha256(json.dumps(before,sort_keys=True).encode()).hexdigest(),commands=runs,selected_tests=selected,
            failure=failure,native_process_acceptance=False,sdk_acceptance=False,release_eligible=False))
    print(status,flush=True)
    return 0 if status=='SCOPED_WINDOWS_CAPNP_CODEC_AND_STATE_MACHINE_PASS' else 1

if __name__=='__main__':
    raise SystemExit(main())
