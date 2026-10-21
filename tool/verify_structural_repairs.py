#!/usr/bin/env python3
"""One repository-relative entry for structural repair checks; never a release gate bypass."""
from __future__ import annotations
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT / 'companions/morrow-codex/derived/c28-cleanup-com-002/codex-rs/windows-sandbox-rs/src'
MANIFEST = 'companions/morrow-codex/derived/c28-cleanup-com-002/codex-rs/windows-sandbox-rs/Cargo.toml'
NATIVE_TARGET = 'build/structural-native-target'
WASM_TARGET = 'build/structural-wasm-target'
WINDOWS_HOST_FIXTURES = (
    ('MORROW_WORKBENCH_WASM', 'plugins/workbench/Cargo.toml', 'morrow_workbench_plugin.wasm'),
    ('MORROW_HTTP_FORWARD_WASM', 'plugins/http_forward/Cargo.toml', 'morrow_http_forward_plugin.wasm'),
    ('MORROW_SERVICE_OUTBOUND_WASM', 'workbench_host/tests/fixtures/service-outbound/Cargo.toml',
     'morrow_service_outbound_test_guest.wasm'),
)
L0_TEST = ('asuser_a1_native::login_launch_contract_windows::'
           'fresh_standard_account_produces_minimal_launch_token')
REQUIRED = ('L0_TOKEN', 'L1_RUNNER', 'L2_BUSINESS_AND_RETIREMENT', 'L3_FULL47_NINE',
            'EDITOR_PERSISTENCE', 'SDK26', 'G04', 'SDK_FREEZE')

def revision() -> str:
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()

def source_digest() -> str:
    paths = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).split(b'\0')
    extra = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '-z'], cwd=ROOT).split(b'\0')
    digest = hashlib.sha256()
    for raw in sorted(set(paths + extra)):
        if not raw:
            continue
        path = ROOT / os.fsdecode(raw)
        digest.update(len(raw).to_bytes(4,'little'));digest.update(raw)
        # Include fixture/config/build-script inputs too; never follow a symlink
        # out of the checkout while measuring repository content.
        if path.is_symlink():
            digest.update(b'LINK'+os.fsencode(os.readlink(path)))
        else:
            digest.update(hashlib.sha256(path.read_bytes()).digest() if path.is_file() else b'MISSING')
    return digest.hexdigest()

def remaining_gates(receipts: list[dict], commit: str, digest: str) -> list[str]:
    """Only exact-source, complete, independently named tiers can close gates."""
    passed = {row.get('gate') for row in receipts
              if row.get('commit') == commit and row.get('source_digest') == digest
              and row.get('status') == 'PASS' and row.get('complete') is True}
    return [gate for gate in REQUIRED if gate not in passed]

def l0_discovered(evidence: str) -> bool:
    """Require the actual libtest entry before any account fixture is run."""
    entries = re.findall(r'^(.+): test\s*$', evidence, re.MULTILINE)
    summaries = re.findall(r'^\d+ tests?, \d+ benchmarks?\s*$', evidence, re.MULTILINE)
    return entries == [L0_TEST] and [line.strip() for line in summaries] == ['1 test, 0 benchmarks']

def l0_executed(evidence: str) -> bool:
    """A zero exit or another one-test suite cannot certify this fixture."""
    entries = re.findall(r'^test (\S+) \.\.\. (\S+)\s*$', evidence, re.MULTILINE)
    summaries = re.findall(r'^test result: (.+)$', evidence, re.MULTILINE)
    return (entries == [(L0_TEST, 'ok')] and len(summaries) == 1
            and re.match(r'ok\. 1 passed; 0 failed; 0 ignored; 0 measured; '
                         r'\d+ filtered out;(?: finished in .+)?\s*$', summaries[0]) is not None)

def command_environment(output: Path) -> dict[str, str]:
    # Freeze fixtures inspect ancestor Cargo/toolchain configuration. The OS
    # profile temp directory inherits a developer's .cargo and is not isolated.
    # Give this run its own temporary root without relaxing those guards or
    # changing the user's HOME, toolchain, dependency locks or Cargo cache.
    temporary = output / 'tmp'
    temporary.mkdir()
    environment = os.environ.copy()
    for name in ('TMPDIR', 'TMP', 'TEMP'):
        environment[name] = str(temporary)
    return environment

def fixture_environment(environment: dict[str, str], output: Path) -> dict[str, str]:
    # Python gate tests use synthetic compiler identities and disposable source
    # archives. Give only those tests an empty Cargo home. Real Cargo commands
    # must retain the actual host configuration and dependency cache.
    result = environment.copy()
    cargo_home = output / 'fixture-cargo-home'
    cargo_home.mkdir(exist_ok=True)
    result['CARGO_HOME'] = str(cargo_home)
    return result

def commands(mode: str, output: Path, disposable: bool = False) -> list[tuple[str,list[str]]]:
    if mode == 'portable':
        result = [(name, [sys.executable,'-m','unittest','discover','-s','tool/tests','-p',name,'-q'])
                  for name in ('test_session_exec_r2.py','test_codex_sdk_candidate.py',
                               'test_sdk_contract_sync.py','test_structural_repairs.py')]
        for module in ('asuser_a1_target','asuser_attempt_outcome'):
            binary = output / (module + ('.exe' if os.name == 'nt' else ''))
            result += [(module+' compile', ['rustc','--edition','2024','--test',str(NATIVE/(module+'.rs')),'-o',str(binary)]),
                       (module, [str(binary)])]
        if os.name == 'nt':
            for name, manifest, _ in WINDOWS_HOST_FIXTURES:
                result.append((name+' build', ['cargo','build','--locked','--manifest-path',manifest,
                               '--target','wasm32-unknown-unknown','--release','--target-dir',WASM_TARGET]))
        result.append(('host unit tests',['cargo','test','--locked','--manifest-path','workbench_host/Cargo.toml','--lib']))
        result.append(('native policy and diagnostics',['cargo','test','--locked','--manifest-path',MANIFEST,
                       '--lib','asuser_','--target-dir',NATIVE_TARGET]))
        return result
    if mode == 'windows':
        if os.name != 'nt':
            raise ValueError('Windows runtime checks require Windows; cross-compilation is not runtime PASS')
        result = [('native A1 regressions',['cargo','test','--locked','--manifest-path',MANIFEST,
                   '-p','codex-windows-sandbox','--target-dir',NATIVE_TARGET,
                   '--lib','asuser_','--','--test-threads=1'])]
        if disposable:
            if os.environ.get('MORROW_DISPOSABLE_WINDOWS_TEST') != '1':
                raise ValueError('Disposable account test requires MORROW_DISPOSABLE_WINDOWS_TEST=1')
            fixture = ['cargo','test','--locked','--manifest-path',MANIFEST,
                       '-p','codex-windows-sandbox','--target-dir',NATIVE_TARGET,'--lib',L0_TEST]
            result.append(('L0_TOKEN_DISCOVERY',fixture + ['--','--ignored','--exact','--list']))
            result.append(('L0_TOKEN',fixture + ['--','--ignored','--exact','--test-threads=1']))
        return result
    raise ValueError(mode)

def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=('portable','windows'),default='portable')
    parser.add_argument('--output',type=Path,default=ROOT/'build/structural-acceptance')
    parser.add_argument('--disposable-windows',action='store_true')
    parser.add_argument('--gate',type=Path,help='Check complete gate receipts instead of running commands')
    args=parser.parse_args(argv)
    head=revision();digest=source_digest()
    if args.gate:
        remaining=remaining_gates(json.loads(args.gate.read_text())['receipts'],head,digest)
        print(json.dumps({'commit':head,'release_eligible':not remaining,'remaining':remaining},indent=2))
        return 2 if remaining else 0
    output=args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error('Output directory is not empty; choose a new directory to preserve prior evidence')
    output.mkdir(parents=True,exist_ok=True)
    steps=commands(args.mode,output,args.disposable_windows)
    environment=command_environment(output)
    synthetic_environment=fixture_environment(environment,output)
    report={'commit':head,'source_digest':digest,'started':dt.datetime.now(dt.timezone.utc).isoformat(),
            'mode':args.mode,'results':[],'receipts':[],'release_eligible':False}
    report_path=output/'result.json'
    for number,(name,command) in enumerate(steps,1):
        print(f'[{number}/{len(steps)}] {name}',flush=True)
        log=output/f'{number:02}.log';started=time.monotonic()
        step_environment=synthetic_environment if name.endswith('.py') else environment
        fixture_pins={}
        if name=='host unit tests' and os.name=='nt':
            step_environment=environment.copy()
            for variable, _, filename in WINDOWS_HOST_FIXTURES:
                path=ROOT/WASM_TARGET/'wasm32-unknown-unknown'/'release'/filename
                data=path.read_bytes()
                if data[:8] != b'\x00asm\x01\x00\x00\x00':
                    raise ValueError('Expected actual compiled Wasm fixture: '+str(path))
                step_environment[variable]=str(path)
                fixture_pins[variable]={'path':str(path),'bytes':len(data),
                                        'sha256':hashlib.sha256(data).hexdigest()}
        try:
            with log.open('w',encoding='utf-8') as stream:
                status=subprocess.run(command,cwd=ROOT,env=step_environment,stdout=stream,stderr=subprocess.STDOUT,timeout=1800).returncode
        except (OSError,subprocess.TimeoutExpired) as error:
            status=-1
            # A timeout is not proof that a dispatched native effect stopped.
            # Preserve all original output, including the first failure.
            with log.open('a',encoding='utf-8') as stream:
                stream.write('\nAcceptance observer failed: '+str(error)+'\n')
        row={'name':name,'command':command,'exit_code':status,'status':'PASS' if status==0 else 'FAIL',
             'seconds':round(time.monotonic()-started,2),'log':str(log)}
        if name.endswith('.py'):
            row['synthetic_cargo_home']=step_environment['CARGO_HOME']
        if fixture_pins:
            row['compiled_fixture_pins']=fixture_pins
        report['results'].append(row)
        if name in ('L0_TOKEN_DISCOVERY','L0_TOKEN') and status==0:
            # --exact + --ignored still exits zero if a test is absent.
            evidence=log.read_text(encoding='utf-8',errors='replace')
            checked = l0_discovered(evidence) if name=='L0_TOKEN_DISCOVERY' else l0_executed(evidence)
            row['test_name']=L0_TEST
            row['verified_test_count']=1 if checked else 0
            if not checked:
                row.update(status='FAIL',exit_code=-1);status=-1
            elif name=='L0_TOKEN':
                report['receipts'].append({'gate':'L0_TOKEN','commit':head,'source_digest':digest,
                                          'complete':True,'status':'PASS'})
        report['remaining']=remaining_gates(report['receipts'],head,digest)
        report_path.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        if status!=0:
            print(f'FAILED: {name}; see {log}',flush=True);return 1
    report['source_unchanged']=revision()==head and source_digest()==digest
    if not report['source_unchanged']:
        report['receipts']=[]
        report['remaining']=list(REQUIRED)
        report_path.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        print('Source changed during acceptance; no gate receipts retained',flush=True)
        return 1
    report_path.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(f'Checks passed; unqualified gates remain: {", ".join(report["remaining"])}')
    return 0

if __name__=='__main__':
    raise SystemExit(main())
