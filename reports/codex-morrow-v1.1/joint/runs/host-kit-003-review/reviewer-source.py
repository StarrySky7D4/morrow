"""Review the explicitly ready host-kit-003. All artifacts/targets stay in joint."""
import argparse
import json
import re
import subprocess
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path
sys.dont_write_bytecode=True
from check_joint import HERE, HOST, BASE, digest, write_json

KIT=HOST/'reports/codex-morrow-v1.1/host/host-kit-003'
PIN='5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01'
SCHEMA='da0ac7a42e4b0f6aec0cfbdd2358cf86688b08e862626186bb9de4914f355a7f'
AUTHORITY=HOST/'contracts/experimental/agent_host_v1'
# Independent expected visible semantics; never generated from the fake's reply.
EXPECT={
 '00-before-hello':['code = denied','stage = "hello-required"'],
 '01-hello':['capabilityBits = 15','qualificationOnly = true'],
 '02-open':['state = prepared','requestFixed = false'],
 '03-read-before-commit':['code = conflict','stage = "read-state"'],
 '04-write':['state = prepared','requestFixed = false'],
 '05-commit':['state = committed','requestFixed = true'],
 '06-first-chunk':['state = streaming','bytes = "first"','transportSequence = 1'],
 '07-final-chunk':['state = eof','bytes = "-second"','transportSequence = 2'],
 '08-append':['durableSequence = 1','replayed = false'],
 '09-append-retry':['durableSequence = 1','replayed = true'],
 '10-append-conflict':['code = conflict','stage = "append-key-bytes"'],
 '11-propose':['state = proposed','execute = false'],
 '12-forged-permit':['code = denied','stage = "permit"'],
 '13-claim':['state = claimed','execute = true'],
 '14-claim-retry':['state = claimed','execute = false'],
 '15-report-unknown':['state = unknown','execute = false'],
 '16-drain':['closingUnconfirmed = true'],
 '17-drained-claim':['code = denied','stage = "draining"'],
 '18-observe-exit':['code = unavailable','stage = "observe-exit"']}

def checked_path(root, relative):
    rel=Path(relative); target=(root/rel).resolve()
    if rel.is_absolute() or '..' in rel.parts or not target.is_relative_to(root.resolve()):
        raise ValueError('unsafe manifest reference: '+relative)
    return target

def check_entries(root, entries):
    seen=set()
    for entry in entries:
        if entry['path'] in seen: raise ValueError('duplicate entry: '+entry['path'])
        seen.add(entry['path']); path=checked_path(root,entry['path']); raw=path.read_bytes()
        if digest(raw)!=entry['sha256'] or len(raw)!=entry['bytes']: raise ValueError('entry mismatch: '+str(path))
    return seen

def main():
    p=argparse.ArgumentParser(description=__doc__); p.add_argument('--run-id',required=True); a=p.parse_args()
    out=(HERE/'runs'/a.run_id).resolve()
    if not out.is_relative_to(HERE/'runs') or out.exists(): p.error('use a fresh joint run directory')
    out.mkdir(parents=True); commands=[]; checks=[]; failures=[]
    (out/'reviewer-source.py').write_bytes(Path(__file__).read_bytes())
    def need(value,message):
        if not value: raise ValueError(message)
    def run(name,args,stdin=None):
        print('Running '+name,flush=True)
        proc=subprocess.run(args,cwd=out,input=stdin,capture_output=True)
        (out/(name+'.stdout.txt')).write_bytes(proc.stdout); (out/(name+'.stderr.txt')).write_bytes(proc.stderr)
        rec={'name':name,'args':args,'cwd':str(out),'exit_code':proc.returncode,
             'stdout_sha256':digest(proc.stdout),'stderr_sha256':digest(proc.stderr)}
        if stdin is not None: rec['stdin_sha256']=digest(stdin)
        commands.append(rec); write_json(out/'commands.json',commands)
        need(proc.returncode==0,name+' failed with exit '+str(proc.returncode))
        return proc.stdout.decode('utf-8',errors='replace')
    try:
        manifest_raw=(KIT/'manifest.json').read_bytes(); need(digest(manifest_raw)==PIN,'ready manifest pin mismatch')
        manifest=json.loads(manifest_raw)
        need(manifest['status']=='complete' and manifest['qualification_only'] is True,'not qualification kit')
        need(Path(manifest['authority']).resolve()==AUTHORITY,'wrong schema authority')
        need(manifest['baseline']['head']==BASE,'wrong host baseline')
        need(manifest['wire_major']==1 and manifest['wire_revision']==1 and manifest['schema_sha256']==SCHEMA,'wrong wire identity')
        files=check_entries(KIT,manifest['files'])
        actual={x.relative_to(KIT).as_posix() for x in KIT.rglob('*') if x.is_file()}
        need(actual==files|{'manifest.json'},'extra or missing kit file')
        check_entries(AUTHORITY,manifest['source_files'])
        need(digest((AUTHORITY/'agent_host.capnp').read_bytes())==SCHEMA,'authority schema differs')
        generated=json.loads((KIT/'generated/manifest.json').read_text(encoding='utf-8'))
        need(generated['source_sha256']==SCHEMA,'generated source digest mismatch')
        check_entries(KIT/'generated',generated['files'])
        crate=tomllib.loads((KIT/'Cargo.toml').read_text(encoding='utf-8'))
        need(crate['features']['default']==[],'fake unexpectedly enabled by default')
        need(set(crate['dependencies'])=={'capnp','sha2'},'unexpected host internal/runtime dependency')
        checks.append({'name':'ready_manifest_authority_inventory','status':'verified','files':len(files),'source_files':len(manifest['source_files'])})
        run('host-verify-kit',[sys.executable,'-B',str(HOST/'tool/agent_host_build.py'),'verify-kit','--input',str(KIT)])
        vm=json.loads((KIT/'vectors/manifest.json').read_text(encoding='utf-8'))
        need(vm['schema_sha256']==SCHEMA and vm['qualification_only'] is True,'vector identity mismatch')
        need({x['name'] for x in vm['vectors']}==set(EXPECT) and len(vm['vectors'])==19,'vector names/count mismatch')
        for vector in vm['vectors']:
            decoded={}
            for direction in ['request','reply']:
                raw=checked_path(KIT/'vectors',vector[direction]).read_bytes()
                need(digest(raw)==vector[direction+'_sha256'],'vector digest mismatch')
                decoded[direction]=run('decode-'+vector['name']+'-'+direction,['capnp','decode',str(KIT/'agent_host.capnp'),'Frame'],raw)
            req,reply=decoded['request'],decoded['reply']
            need('qualificationOnly = true' in reply,'reply lacks qualification marker')
            for key in ['requestId','sessionId','instanceEpoch']:
                a1=re.search(key+r' = ([^,\n]+)',req); b1=re.search(key+r' = ([^,\n]+)',reply)
                need(a1 and b1 and a1.group(1)==b1.group(1),'request/reply correlation mismatch: '+key)
            for token in EXPECT[vector['name']]: need(token in reply,'semantic mismatch '+vector['name']+': '+token)
            if vector['name']=='08-append': need('producerSequence = 9007199254740993' in req,'u64 lost precision')
        need(len(vm['rejections'])==1 and vm['rejections'][0]['error']=='UnsupportedVersion','missing version rejection')
        bad=vm['rejections'][0]
        need(digest(checked_path(KIT/'vectors',bad['request']).read_bytes())==bad['sha256'],'bad-version vector changed')
        checks.append({'name':'independent_vector_decode_and_semantic_expectations','status':'verified','pairs':19,'directions':38,'rejection_vectors':1})
        target=out/'target'
        common=['--locked','--offline','--manifest-path',str(KIT/'Cargo.toml'),'--target-dir',str(target)]
        tests=run('consumer-tests',['cargo','test',*common,'--features','qualification','--message-format=json'])
        need('14 passed; 0 failed; 0 ignored' in tests,'expected 14 tests with zero ignored')
        need('maximum_event_batch_remains_readable_by_consumer_after_validation ... ok' in tests,'missing maximum event batch consumer regression')
        outputs=[json.loads(l) for l in tests.splitlines() if l.startswith('{')]
        dirs=[Path(o['out_dir']) for o in outputs if o.get('reason')=='build-script-executed' and 'morrow-agent-host-contract' in o.get('package_id','')]
        need(bool(dirs),'missing generated consumer bindings')
        for entry in generated['files']:
            need(digest((dirs[-1]/entry['path']).read_bytes())==entry['sha256'],'consumer generated binding differs')
        run('default-no-fake',['cargo','check',*common])
        run('consumer-vectors',['cargo','run',*common,'--features','qualification','--example','vectors','--','--output',str(out/'vectors')])
        vector_files=[x for x in manifest['files'] if x['path'].startswith('vectors/')]
        check_entries(out,vector_files)
        checks.append({'name':'consumer_compile_tests_default_build_and_vector_reproduction','status':'verified','tests':14,'ignored':0,'reproduced_vector_files':len(vector_files)})
        check_entries(KIT,manifest['files']); check_entries(AUTHORITY,manifest['source_files'])
        need(digest((KIT/'manifest.json').read_bytes())==PIN,'kit changed during review')
        checks.append({'name':'inputs_unchanged_after_consumer_run','status':'verified'})
    except Exception as error:
        failures.append(str(error))
    result={'captured_at':datetime.now(timezone.utc).isoformat(),'status':'failed' if failures else 'verified',
      'scope':'M-01 four-family experimental Rust qualification kit only; not full M-01, real transport/store/OS isolation, P-02 or G0',
      'kit_manifest':{'path':str(KIT/'manifest.json'),'sha256':PIN},'schema_sha256':SCHEMA,
      'reviewer_sha256':digest(Path(__file__).read_bytes()),'checks':checks,'failures':failures,
      'commands':'commands.json','g0_status':'blocked','product_scenarios_verified':0}
    write_json(out/'result.json',result); print(json.dumps(result,ensure_ascii=False,indent=2)); return 1 if failures else 0

if __name__=='__main__': sys.exit(main())
