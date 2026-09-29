"""Authorized sixth-round replay; reuse the separate completed read-only audit."""
from pathlib import Path
import sys, os, json, subprocess, shutil, traceback
sys.dont_write_bytecode=True
import review_p02_integration as audit
HERE=audit.HERE
RUN=HERE/'runs/p02-integration-004-replay-001'
def normalize(value):
    if isinstance(value,list): return [normalize(x) for x in value]
    if not isinstance(value,dict): return value
    out={k:normalize(v) for k,v in value.items()}
    if 'body_utf8' in value:
        out['body_utf8']=json.loads(value['body_utf8'])
        out['body_sha256']='<verified raw bytes; JSON object order normalized>'
    if value.get('method')=='ExecBackend::start':
        out['params']['processId']='<validated original Core UUID v4>'
        out['params_sha256']='<independently recomputed UUID-dependent parameters>'
        out['proposal_sha256']='<UUID-dependent proposal digest; no independent wire decode>'
    return out
def main():
    RUN.mkdir(parents=True,exist_ok=False); shutil.copyfile(__file__,RUN/'reviewer-source.py')
    result=dict(status='failed',independent_compilation=False,independent_execution=True,P02='blocked',J00='blocked',G0='blocked',product_graphs='0/2',product_acceptance_verified=0,product_acceptance_not_run=84)
    try:
        review_path=HERE/'runs/p02-integration-004-readonly-001/result.json'; reviewed=audit.read(review_path)
        audit.require(reviewed['status']=='verified_read_only','read-only preflight unavailable')
        result['read_only_evidence']=dict(path=str(review_path),sha256=audit.digest(review_path))
        before=audit.snapshot(); audit.write(RUN/'inputs-before.json',before)
        audit.require(audit.source_review()==reviewed['sources'],'source drift since read-only audit')
        h=audit.read(audit.PLUGIN/audit.BATCH/'handoff.json'); exe=audit.PLUGIN/h['artifact']
        profile=RUN/'profile'; temp=RUN/'tmp'
        for p in (profile/'AppData/Local',profile/'AppData/Roaming',temp): p.mkdir(parents=True,exist_ok=False)
        env={}
        for key in ('SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'):
            v=os.environ.get(key)
            if v is not None: env[key]=v
        env.update(PATH=str(Path(env['SYSTEMROOT'])/'System32'),HOME=str(profile),USERPROFILE=str(profile),LOCALAPPDATA=str(profile/'AppData/Local'),APPDATA=str(profile/'AppData/Roaming'),TEMP=str(temp),TMP=str(temp),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_SYSTEM=os.devnull,GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0')
        output=RUN/'runtime.json'; argv=[str(exe),str(output)]
        audit.require(audit.digest(exe)==h['artifact_sha256'],'exe drift before replay')
        result.update(argv=argv,exe_sha256=audit.digest(exe),environment_keys=sorted(env),environment_values_enumerated=False,profile=str(profile),temporary=str(temp))
        audit.write(RUN/'pre-run.json',result)
        executed=subprocess.run(argv,cwd=RUN,env=env,capture_output=True,timeout=30)
        (RUN/'stdout.txt').write_bytes(executed.stdout); (RUN/'stderr.txt').write_bytes(executed.stderr)
        result['exit_code']=executed.returncode; audit.require(executed.returncode==0,'probe exit failure')
        evidence=audit.runtime_review(dict(h,runtime=str(output),runtime_sha256=audit.digest(output)))
        actual=audit.read(output); producer=audit.read(audit.PLUGIN/h['runtime'])
        audit.require(normalize(actual)==normalize(producer),'runtime semantic difference')
        audit.require(not (temp/'restricted-never-created-store').exists(),'fixture storage directory was created')
        after=audit.snapshot(); audit.write(RUN/'inputs-after.json',after); audit.require(before==after,'input drift after replay')
        audit.require(audit.source_review()==reviewed['sources'],'source/shared dependency drift after replay')
        result.update(status='verified_limited',cases=24,counted_assertions=109,input_count=len(before),frozen_inputs_unchanged=True,runtime_sha256=audit.digest(output),producer_runtime_sha256=h['runtime_sha256'],runtime_bytes_equal=output.read_bytes()==(audit.PLUGIN/h['runtime']).read_bytes(),semantic_equal=True,HTTP_bodies_rehashed=evidence['HTTP_bodies_rehashed'],fixture_store_directory_absent=True,owner_release_only=True,host_writer_release_verified=False,OS_monitoring=False,normal_product_build='not_run',comparison='Raw HTTP lengths/hashes and exec params digests independently checked before normalizing JSON object order, UUID process handles and dependent proposal hashes; dynamic proposal wire not independently decoded.')
    except Exception as error:
        result['error']=str(error); (RUN/'exception.txt').write_text(traceback.format_exc(),encoding='utf-8')
    audit.write(RUN/'result.json',result)
    print(json.dumps(result,ensure_ascii=False)); return 0 if result['status']=='verified_limited' else 1
if __name__=='__main__': raise SystemExit(main())
