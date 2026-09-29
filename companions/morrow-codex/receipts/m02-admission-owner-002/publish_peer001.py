"""Freeze a locally checked test peer, with no claim of real host qualification."""
from pathlib import Path
import argparse, json, shutil, sys
sys.dont_write_bytecode = True
from run_local import ROOT, HERE, SOURCE, OUT, sha, read, frozen, snapshot

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['build','test','offline_cli','acl_receipt']: parser.add_argument('--'+name.replace('_','-'),required=True,type=Path)
    args=parser.parse_args()
    build=read(args.build); tests=read(args.test); cli=read(args.offline_cli)
    for stage,data in [('build',build),('test',tests),('offline-cli',cli)]:
        assert data['status']=='passed_local_'+stage
        assert data['inputs_before']==data['inputs_after']
        # Source/compiler/lock identity must match all stages; ancillary ACL-script
        # hardening after tests is not retroactively described as test execution.
        for name,want in data['inputs_after'].items():
            if name.startswith('native/') or name.endswith('run_local.py') or name.endswith('config.toml'):
                assert sha(ROOT/name)==want,name
    for name,want in build['inputs_after'].items(): assert sha(ROOT/name)==want,name
    original=Path(build['artifact']['path']); assert sha(original)==build['artifact']['sha256']==cli['artifact_sha256']
    baseline=frozen()
    client=ROOT/'receipts/m02-native-session-001/candidate-001.json'
    assert sha(client)=='9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'
    inputs=snapshot()
    for name,want in read(client)['input_sha256'].items(): assert sha(ROOT/name)==want; inputs[name]=want
    candidate=OUT/'candidates/replay-peer-001'; candidate.mkdir(parents=True,exist_ok=False)
    exe=candidate/'morrow-session-replay-peer.exe'; shutil.copyfile(original,exe)
    bindings=list((OUT/'target/x86_64-pc-windows-msvc/debug/build').glob('morrow-native-session-wire-*/out/native_session_capnp.rs'))
    assert bindings and all(sha(p)=='2475cee0b5cc676fe36c637daf79eb0fafd4ac378478ac7c076612fe76bc5fc9' for p in bindings)
    binding=candidate/'native_session_capnp.rs'; shutil.copyfile(bindings[0],binding)
    for run_result in [args.build,args.test,args.offline_cli]:
        for p in run_result.parent.iterdir():
            if p.is_file(): inputs[p.relative_to(ROOT).as_posix()]=sha(p)
    for p in [exe,binding,client,args.acl_receipt]: inputs[p.relative_to(ROOT).as_posix()]=sha(p)
    assert read(args.acl_receipt)['status']=='materials_ready_no_authority'
    record={
        'status':'ready_test_peer_local_checks_only_real_host_pair_pending',
        'exe':str(exe),'exe_sha256':sha(exe),
        'ordinary_client_reused_without_rebuild':True,'ordinary_client_manifest':str(client),'ordinary_client_manifest_sha256':sha(client),
        'ordinary_client_exe_sha256':'068122a87c1bbfe6cbac42686aa7e9abb2c520b54075de47c1d5bb0da6eb525b',
        'wire':'unchanged host capnp-kit-001 major2 revision1','schema_sha256':'fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450',
        'scenarios':['capture','replay-hello','replay-query','hold-output'],
        'args':['--morrow-native-session-v2','--scenario','SCENARIO','--evidence-dir','ABS','[--capture-dir ABS for replay]'],
        'observation_delay_ms':150,'watchdog_ms':10000,'holder_lifetime_ms':2000,
        'tests':str(args.test),'offline_cli':str(args.offline_cli),'build':str(args.build),
        'input_sha256':inputs,'frozen_previous_inputs':baseline,
        'limits':['Test peer is not a production client or approval authority','Capture completion is not process-exit or pipe-EOF proof','Trusted harness must witness first host/child exit and both EOFs before replay','Raw bytes are peer evidence; compare to host capture and independently decode','All raw test frames stay in ACL-restricted batch materials directories','No A/B role interchange coverage claimed','No new host runtime pairing or independent acceptance yet','M02 partial, G0 blocked, product graphs0/2,84not_run; no commit/push/release']}
    path=HERE/'candidate-001.json'
    with path.open('x',encoding='utf-8') as stream: stream.write(json.dumps(record,indent=2)+'\n')
    print(json.dumps({'manifest':str(path),'manifest_sha256':sha(path),'exe':str(exe),'exe_sha256':sha(exe),'inputs':len(inputs),'frozen_previous_inputs':len(baseline)}))
if __name__=='__main__': main()
