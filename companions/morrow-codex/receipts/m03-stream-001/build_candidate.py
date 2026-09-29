"""Offline pre-wire compilation/local unit checks only; no HTTP/native launch.

Each invocation preserves logs and before/after hashes. Frozen 004 supplies the
exact source graph/lock seed, never a writable target. No version selection or
download is allowed. Lock is the only stage permitted to change the new lock.
"""
from pathlib import Path
import argparse
import datetime as dt
import hashlib
import json
import os
import shutil
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = ROOT / 'receipts/m03-stream-001'
PROBE = ROOT / 'qualification/m03-stream-001'
OUT = ROOT / 'out/m03-stream-001'
SOURCE = ROOT / 'upstream/p02-integration-004/codex-work'
TOOLCHAIN = Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc')

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for part in iter(lambda: f.read(1024 * 1024), b''):
            h.update(part)
    return h.hexdigest()

def load(path):
    return json.loads(path.read_text(encoding='utf-8'))

def checked(path, expected):
    if digest(path) != expected:
        raise RuntimeError('Frozen hash mismatch: ' + str(path))
    return load(path)

def frozen(complete=False):
    handoff = checked(ROOT / 'receipts/p02-integration-004/handoff.json',
        '513b1cfbf4b19b32c247b06810430ae29b1e1b779078b9761100a86c7a8d1751')
    result = {}
    for name, expected in handoff['input_sha256'].items():
        actual = digest(ROOT / name)
        if actual != expected:
            raise RuntimeError('Frozen 004 input mismatch: ' + name)
        result[name] = actual
    # Full source identity was checked during lock preparation. Subsequent
    # compilation stages verify the 83 sealed 004 inputs, without rehashing the
    # whole old checkout or claiming a new independent provenance audit.
    if not complete:
        return result
    baseline = checked(ROOT / 'receipts/p02-source-batch-001/codex-content-manifest.json',
        '577253fb4cacc1bc292fb46db725d713c51368228ebb7ae620a65b397c5df2b7')
    patch = checked(ROOT / 'receipts/p02-integration-004/patch-after-build-001.json',
        'c776765cb99e22c03a725a2dca7ac11835166c24db1aeb8a8f510228e90243dc')
    expected = {f['path']: f['sha256'] for f in baseline['files']}
    symlinks = {f['path'] for f in baseline['files'] if f['mode'] == '120000'}
    expected.update({f['path']: f['after_sha256'] for f in patch['changes']})
    for name, sha in expected.items():
        path = SOURCE / name
        # Git hashes link text, not the target's contents. Targets are separately
        # present in the same frozen source manifest; never rewrite the link.
        actual = hashlib.sha256(os.readlink(path).encode()).hexdigest() if name in symlinks else digest(path)
        if actual != sha:
            raise RuntimeError('Frozen source mismatch: ' + name)
        result[path.relative_to(ROOT).as_posix()] = actual
    return result

def inputs():
    paths = [Path(__file__), PROBE/'Cargo.toml', PROBE/'Cargo.lock', PROBE/'README.md', OUT/'cargo-home/config.toml']
    paths += sorted((PROBE/'src').rglob('*.rs'))
    return {p.relative_to(ROOT).as_posix(): digest(p) if p.exists() else None for p in paths}

def environment(run):
    allowed = {'SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)',
               'PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'}
    env = {k:v for k,v in os.environ.items() if k.upper() in allowed}
    profile, temp = OUT/'isolated-userprofile', run/'tmp'
    for path in (profile, profile/'AppData/Local', profile/'AppData/Roaming', temp):
        path.mkdir(parents=True, exist_ok=True)
    env.update({'PATH': os.pathsep.join([str(TOOLCHAIN/'bin'), r'C:\Users\Administrator\capnp-bin',
        r'C:\Windows\System32', r'C:\Windows', r'C:\Windows\System32\Wbem', r'C:\Windows\System32\WindowsPowerShell\v1.0']),
        'CARGO_HOME':str(OUT/'cargo-home'), 'CARGO_TARGET_DIR':str(OUT/'target'), 'CARGO_NET_OFFLINE':'true',
        'CARGO_TERM_COLOR':'never', 'CARGO_INCREMENTAL':'0', 'CARGO_BUILD_JOBS':'2',
        'CARGO_PROFILE_DEV_DEBUG':'0', 'CARGO_PROFILE_TEST_DEBUG':'0',
        'RUSTC':str(TOOLCHAIN/'bin/rustc.exe'), 'RUSTDOC':str(TOOLCHAIN/'bin/rustdoc.exe'),
        'RUSTUP_AUTO_INSTALL':'0','HOME':str(profile),'USERPROFILE':str(profile),
        'LOCALAPPDATA':str(profile/'AppData/Local'),'APPDATA':str(profile/'AppData/Roaming'),
        'TEMP':str(temp),'TMP':str(temp),'GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_SYSTEM':os.devnull,
        'GIT_CONFIG_GLOBAL':os.devnull,'GIT_TERMINAL_PROMPT':'0','GIT_NO_LAZY_FETCH':'1','VSCMD_SKIP_SENDTELEMETRY':'1'})
    commands = []
    vswhere = r'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
    discovery = subprocess.run([vswhere,'-latest','-products','*','-requires','Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        '-property','installationPath'], cwd='C:/',env=env,capture_output=True,timeout=30,check=True)
    script = Path(discovery.stdout.decode('utf-8-sig').strip())/'Common7/Tools/VsDevCmd.bat'
    if not script.is_file() or any(c in str(script) for c in '\"&|<>^%\r\n'): raise RuntimeError('Invalid VS script path')
    # Avoid list2cmdline escaping embedded quotes into cmd's own command text.
    batch = run/'vs-env.cmd'
    batch.write_text(f'@echo off\ncall "{script}" -no_logo -arch=x64 -host_arch=x64\nif errorlevel 1 exit /b 1\nset\n',encoding='ascii')
    vs = subprocess.run([r'C:\Windows\System32\cmd.exe','/d','/c',str(batch)], cwd='C:/',env=env,capture_output=True,timeout=60)
    # Never persist `set` output or inherited environment values.
    diagnostics = [s for s in vs.stdout.decode('utf-8',errors='replace').splitlines() if '=' not in s]
    (run/'vs-diagnostics.txt').write_text('\n'.join(diagnostics)+'\n'+vs.stderr.decode('utf-8',errors='replace'), encoding='utf-8')
    commands.append({'purpose':'isolated VS environment','exit_code':vs.returncode})
    if vs.returncode: raise RuntimeError('VsDevCmd failed: '+str(run/'vs-diagnostics.txt'))
    permitted = {'PATH','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR',
        'WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION','VSCMD_ARG_TGT_ARCH','VSCMD_ARG_HOST_ARCH'}
    for line in vs.stdout.decode('utf-8',errors='replace').splitlines():
        key, sep, value = line.partition('=')
        if sep and key.upper() in permitted: env[key.upper()] = value
    for label, args in [('rustc',['-vV']), ('cargo',['--version'])]:
        version = subprocess.run([str(TOOLCHAIN/'bin'/(label+'.exe')), *args], cwd='C:/', env=env,
            capture_output=True, timeout=30, check=True).stdout.decode('utf-8')
        if ('release: 1.95.0' if label == 'rustc' else 'cargo 1.95.0 ') not in version:
            raise RuntimeError('Toolchain version mismatch')
        commands.append({'tool': label, 'version': version.strip()})
    return env, commands

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['lock','build','test'])
    args = parser.parse_args()
    stamp = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    receipt = RECEIPTS/'runs'/(args.stage+'-'+stamp)
    run = OUT/'runs'/stamp
    receipt.mkdir(parents=True, exist_ok=False)
    run.mkdir(parents=True, exist_ok=False)
    result = {'stage':args.stage,'status':'blocked','network':'offline_no_native_or_http',
        'product_success_claimed':False,'old_refusal_suite_rerun':False}
    before = source_before = None
    try:
        for config in (Path('C:/.cargo/config'), Path('C:/.cargo/config.toml')):
            if config.exists(): raise RuntimeError('Unexpected root Cargo config')
        source_before = frozen(complete=args.stage == 'lock')
        (receipt/'frozen-before.json').write_text(json.dumps(source_before, indent=2)+'\n', encoding='utf-8')
        home = OUT/'cargo-home'
        home.mkdir(parents=True, exist_ok=True)
        expected_config = (ROOT/'out/p02-integration-004/cargo-home/config.toml').read_bytes()
        config = home/'config.toml'
        if config.exists() and config.read_bytes() != expected_config: raise RuntimeError('New Cargo configuration drift')
        if not config.exists(): config.write_bytes(expected_config)
        if args.stage == 'lock' and not (PROBE/'Cargo.lock').exists():
            shutil.copyfile(ROOT/'qualification/p02-integration-004/Cargo.lock', PROBE/'Cargo.lock')
        before = inputs()
        env, result['tool_commands'] = environment(run)
        base = [str(TOOLCHAIN/'bin/cargo.exe')]
        manifest = ['--manifest-path',str(PROBE/'Cargo.toml')]
        if args.stage == 'lock':
            argv = base+['metadata','--offline','--format-version','1']+manifest
        else:
            argv = base+[args.stage,'--offline','--locked','--target','x86_64-pc-windows-msvc']+manifest
            argv += ['--message-format=json-render-diagnostics'] if args.stage == 'build' else ['--lib','--','--test-threads=1']
        result.update({'argv':argv,'environment_keys':sorted(env),'target':str(OUT/'target')})
        print(json.dumps({'stage':args.stage,'receipt':str(receipt),'state':'starting'}), flush=True)
        started = time.monotonic()
        with (receipt/'stdout.txt').open('wb') as stdout, (receipt/'stderr.txt').open('wb') as stderr:
            process = subprocess.Popen(argv,cwd='C:/',env=env,stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr)
            result['pid'] = process.pid
            try:
                code = process.wait(timeout=3600)
            except subprocess.TimeoutExpired:
                # Only this owned build tree; never stop another chat/service.
                subprocess.run([r'C:\Windows\System32\taskkill.exe','/PID',str(process.pid),'/T','/F'],
                    capture_output=True, timeout=30, check=False)
                raise RuntimeError('Owned build exceeded 3600 seconds')
        result.update({'exit_code':code,'elapsed_seconds':time.monotonic()-started})
        if code: raise RuntimeError('Stage failed; diagnostics preserved')
        result['status'] = {'lock':'lock_prepared_not_build_proof','build':'compiled_pre_wire_only',
                            'test':'passed_local_unit_only'}[args.stage]
    except Exception as error:
        result['error'] = str(error)
    finally:
        try:
            after = frozen(complete=args.stage == 'lock')
            result['frozen_count'] = len(after)
            result['frozen_unchanged'] = source_before == after
            if source_before is not None and source_before != after: raise RuntimeError('Frozen inputs changed')
            result['inputs_before'], result['inputs_after'] = before, inputs()
            changes = [p for p in before or {} if before[p] != result['inputs_after'].get(p)]
            result['changed_inputs'] = changes
            allowed = {'qualification/m03-stream-001/Cargo.lock'} if args.stage == 'lock' else set()
            if set(changes)-allowed: raise RuntimeError('Input changed during stage')
        except Exception as error:
            result['status'], result['verification_error'] = 'blocked', str(error)
        result['log_sha256'] = {p.name:digest(p) for p in receipt.glob('*.txt')}
        result['completed_utc'] = dt.datetime.now(dt.timezone.utc).isoformat()
        (receipt/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'status':result['status'],'receipt':str(receipt/'result.json'),'error':result.get('error'),
                          'verification_error':result.get('verification_error')}),flush=True)
    return 2 if result['status'] == 'blocked' else 0

if __name__ == '__main__':
    raise SystemExit(main())
