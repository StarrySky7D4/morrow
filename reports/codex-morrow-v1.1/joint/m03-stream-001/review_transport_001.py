"""Bounded independent execution of the sealed six transport tests; no compilation."""
import ctypes
from ctypes import wintypes
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import tomllib

ROOT = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
HOST = ROOT / 'reports/codex-morrow-v1.1/host/m03-stream-001'
KIT = HOST / 'transport-kit-001'
HERE = Path(__file__).resolve().parent
RUN = HERE / 'transport-review-001'

def sha(path):
    return hashlib.file_digest(open(path, 'rb'), 'sha256').hexdigest()

def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

def save(path, obj):
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

def pins():
    wanted = {}
    for rel, digest in manifest['files'].items():
        wanted[str(KIT / rel)] = digest
    for rel, digest in manifest['inputs'].items():
        wanted[str(ROOT / rel)] = digest
    for rel, digest in receipt['old_before'].items():
        wanted[str(ROOT / rel)] = digest
    for rel, digest in receipt['test_artifacts'].items():
        wanted[str(ROOT / rel)] = digest
    for rel, digest in manifest['source_files'].items():
        wanted[str(ROOT / 'network_node_stream_001' / rel)] = digest
    for path, digest in fixed.items():
        wanted[str(path)] = digest
    return wanted

def check_inputs(label):
    actual = {p: sha(p) for p in expected}
    bad = [p for p in expected if expected[p] != actual[p]]
    save(RUN / f'identities-{label}.json', {'hashes': actual, 'mismatches': bad})
    assert not bad, bad
    return len(actual)

def process_identity(proc):
    # Only the exact process handle returned by our Popen; no process enumeration.
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    handle = wintypes.HANDLE(int(proc._handle))
    get_times = kernel.GetProcessTimes
    get_times.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    get_times.restype = wintypes.BOOL
    times = [wintypes.FILETIME() for _ in range(4)]
    if not get_times(handle, *(ctypes.byref(t) for t in times)):
        raise ctypes.WinError(ctypes.get_last_error())
    query = kernel.QueryFullProcessImageNameW
    query.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    query.restype = wintypes.BOOL
    buf = ctypes.create_unicode_buffer(32768)
    size = wintypes.DWORD(len(buf))
    if not query(handle, 0, buf, ctypes.byref(size)):
        raise ctypes.WinError(ctypes.get_last_error())
    return {'pid': proc.pid, 'image_path': buf.value, 'image_sha256': sha(buf.value),
            'creation_filetime': (times[0].dwHighDateTime << 32) | times[0].dwLowDateTime,
            'method': 'held Popen handle, QueryFullProcessImageNameW, GetProcessTimes'}

assert not RUN.exists(), 'Do not overwrite an existing run'
RUN.mkdir()
fixed = {
    HOST / 'transport-handoff-001.json': 'f069be46cea410f6e7538c6d217020f77f7d14b79f9d76e6e4b08609d362f727',
    KIT / 'manifest.json': 'c19503dbb991377fd7df1a66618ab4a19c7a8658fc7e78529349ac5bce16abec',
    HOST / 'transport-20260928T210834849150Z/result.json': '0c4b74cc41dc39455869d5e8003eca336728a44c52b9b8ec9247cd8f21a28db2',
}
for path, digest in fixed.items():
    assert sha(path) == digest, str(path)
manifest = read(KIT / 'manifest.json')
receipt = read(HOST / 'transport-20260928T210834849150Z/result.json')
assert receipt['before'] == receipt['after']
assert receipt['old_before'] == receipt['old_after']
expected = pins()
before_count = check_inputs('before')
source = ROOT / 'network_node_stream_001'
origin = read(source / 'provenance/origin.json')
for rel, digest in origin['origins'].items():
    assert sha(ROOT / rel) == digest
    assert sha(source / 'provenance' / rel.replace('/', '__')) == digest

old_client_tests = (ROOT / 'network_node/tests/client.rs').read_text(encoding='utf-8')
new_client_tests = (source / 'tests/client.rs').read_text(encoding='utf-8')
assert new_client_tests == old_client_tests.replace('morrow_network_node::', 'morrow_network_node_stream::')

oldlock = tomllib.loads((source / 'provenance/network_node__Cargo.lock').read_text(encoding='utf-8-sig'))
newlock = tomllib.loads((source / 'Cargo.lock').read_text(encoding='utf-8-sig'))
old_packages = {(p['name'], p['version'], p.get('source')): p.get('checksum') for p in oldlock['package']}
new_packages = [p for p in newlock['package'] if p.get('source')]
lock_added = [p['name'] + '@' + p['version'] for p in new_packages if (p['name'], p['version'], p.get('source')) not in old_packages]
lock_changed = [p['name'] + '@' + p['version'] for p in new_packages if (p['name'], p['version'], p.get('source')) in old_packages and old_packages[(p['name'], p['version'], p.get('source'))] != p.get('checksum')]
assert not lock_changed
patch = ''.join(''.join(difflib.unified_diff((ROOT / ('network_node/' + rel)).read_text(encoding='utf-8').splitlines(keepends=True), (source / rel).read_text(encoding='utf-8').splitlines(keepends=True), fromfile='network_node/' + rel, tofile='network_node_stream_001/' + rel)) for rel in ['src/client.rs', 'src/lib.rs', 'Cargo.toml', 'tests/client.rs'])
assert patch == (source / 'provenance/extraction.patch').read_text(encoding='utf-8')
(RUN / 'independent-extraction.patch').write_text(patch, encoding='utf-8')

first = read(HOST / 'transport-20260928T210721765539Z/result.json')
changed_after_failed = [rel for rel, digest in receipt['before'].items() if first['before'].get(rel) != digest]
assert changed_after_failed == ['network_node_stream_001/tests/stream.rs'], changed_after_failed
assert all(sha(KIT / (cmd['name'] + '.' + stream)) == cmd[stream + '_sha256'] for cmd in receipt['commands'] for stream in ['stdout', 'stderr'])
stderr = (KIT / 'tests.stderr').read_text(encoding='utf-8')
assert 'Running tests\\stream.rs' in stderr and 'stream-454c3e399a964f0e.exe' in stderr

test_names = re.findall(r'#\[tokio::test\]\s*async fn (\w+)\(', (source / 'tests/stream.rs').read_text())
assert len(test_names) == 6
exe = RUN / 'stream-454c3e399a964f0e.exe'
shutil.copyfile(KIT / 'tests' / exe.name, exe)
exe_hash = sha(exe)
assert exe_hash == '47886ab6739357522b8fb2c4da78f1b0d091822048a65861ae755518e6104d54'
env = {key: os.environ[key] for key in ('SystemRoot', 'WINDIR', 'COMSPEC') if key in os.environ}
for key, folder in [('TEMP', 'tmp'), ('TMP', 'tmp'), ('USERPROFILE', 'profile'), ('APPDATA', 'profile/roaming'), ('LOCALAPPDATA', 'profile/local')]:
    value = RUN / folder
    value.mkdir(parents=True, exist_ok=True)
    env[key] = str(value)
env['PATH'] = str(Path(env['SystemRoot']) / 'System32')
save(RUN / 'execution-inputs.json', {'runner_sha256': sha(__file__), 'exe_sha256': exe_hash, 'test_names': test_names, 'environment': env, 'build': False})
results = []
for index, name in enumerate(test_names, 1):
    case_dir = RUN / f'case-{index:02d}'
    case_dir.mkdir()
    args = [str(exe), '--exact', name, '--nocapture', '--test-threads=1']
    start = time.monotonic_ns()
    with open(case_dir / 'stdout.txt', 'wb') as out, open(case_dir / 'stderr.txt', 'wb') as err:
        proc = subprocess.Popen(args, cwd=case_dir, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err, creationflags=subprocess.CREATE_NO_WINDOW)
        identity = process_identity(proc)
        timed_out = False
        try:
            code = proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            timed_out = True
            proc.kill()  # Only this runner's exact owned test process.
            code = proc.wait(timeout=5)
    stdout = (case_dir / 'stdout.txt').read_text(encoding='utf-8')
    result = {'name': name, 'args': args, 'process': identity, 'exit_code': code, 'timed_out': timed_out,
              'elapsed_ns': time.monotonic_ns() - start, 'stdout_sha256': sha(case_dir / 'stdout.txt'),
              'stderr_sha256': sha(case_dir / 'stderr.txt'),
              'passed': code == 0 and '1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out' in stdout}
    save(case_dir / 'result.json', result)
    results.append(result)
    print(json.dumps({'case': name, 'passed': result['passed'], 'exit_code': code}), flush=True)
after_count = check_inputs('after')
assert sha(exe) == exe_hash
summary = {'status': 'independent_fixed_transport_tests_passed' if all(r['passed'] for r in results) else 'independent_execution_failed',
           'passed': sum(r['passed'] for r in results), 'total': 6, 'results': results,
           'verified_inputs_before_after': [before_count, after_count], 'old_inputs_unchanged': len(receipt['old_before']),
           'provenance_originals': len(origin['origins']), 'inherited_tests_only_package_rename': True,
           'patch_independently_reconstructed': True, 'new_lock_registry_packages': len(new_packages),
           'registry_packages_added_vs_old_lock': lock_added, 'registry_checksums_changed': lock_changed,
           'changed_after_failed_attempt': changed_after_failed,
           'binary_binding': 'source/lock pre-post hashes plus producer cargo compile/run log and receipt artifact hashes; no independent build or compiler JSON artifact graph',
           'independent_build': False, 'old_policy_tests_executed': False,
           'limits': ['transport tests only', 'producer-authored fixtures independently executed, no external packet observer', 'no native IPC/Core/SSE/OS pending/TLS/public network qualification'],
           'product_pass_credit': 0}
save(RUN / 'result.json', summary)
print(json.dumps({'status': summary['status'], 'passed': summary['passed'], 'inputs': before_count, 'lock_added': lock_added}), flush=True)
