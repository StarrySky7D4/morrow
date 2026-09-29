"""Verify the delivered M-00 inputs; output remains exclusively under joint."""
import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
sys.dont_write_bytecode = True
from check_joint import HERE, HOST, PLAN, BASE, digest, write_json

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run-id',required=True)
    a=p.parse_args()
    out=(HERE/'runs'/a.run_id).resolve()
    if not out.is_relative_to(HERE/'runs') or out.exists(): p.error('use a fresh joint run directory')
    source=HOST/'reports/codex-morrow-v1.1/host/m00-inputs.json'
    raw=source.read_bytes(); doc=json.loads(raw)
    failures=[]; entries=[]
    if digest(raw)!='391d252a7a9bbc8ef7d76222ed5a043b4a267b61fe3ba7c4ca4088eefb43d5e7': failures.append('M-00 handoff digest changed; new explicit handoff required')
    if doc['head']!=BASE or doc['branch']!='codex/io-safety-refactor' or Path(doc['root']).resolve()!=HOST: failures.append('M-00 baseline identity mismatch')
    for group,base in [('legacy_inputs',HOST),('plan_inputs',PLAN)]:
        seen=set()
        for entry in doc[group]:
            path=(base/entry['path']).resolve()
            if not path.is_relative_to(base) or path in seen:
                failures.append('unsafe or duplicate input '+str(path)); continue
            seen.add(path); content=path.read_bytes()
            result={'path':str(path),'match':digest(content)==entry['raw_sha256'],'size_match':len(content)==entry['bytes']}
            if 'lf_sha256' in entry: result['lf_match']=digest(content.replace(b'\r\n',b'\n'))==entry['lf_sha256']
            if not all(v for k,v in result.items() if k!='path'): failures.append('input mismatch '+str(path))
            entries.append(result)
    if len(doc['legacy_inputs'])!=178 or len(doc['plan_inputs'])!=7: failures.append('handoff input count mismatch')
    checks=[]
    for script,args in [('tool/verify_plugin_sdk_baseline.py',[]),('tool/sync_plugin_sdk_contracts.py',['--check'])]:
        cmd=[sys.executable,'-B',str(HOST/script),*args]
        completed=subprocess.run(cmd,cwd=HOST,capture_output=True,text=True,encoding='utf-8',errors='replace')
        checks.append({'command':cmd,'source_sha256':digest((HOST/script).read_bytes()),'exit_code':completed.returncode,'stdout':completed.stdout,'stderr':completed.stderr})
        if completed.returncode: failures.append('read-only SDK check failed: '+script)
    result={'captured_at':datetime.now(timezone.utc).isoformat(),'status':'failed' if failures else 'verified',
            'scope':'M-00 baseline handoff input integrity and old SDK original identity; not guest runtime or M-01 backend qualification',
            'input_manifest':{'path':str(source),'sha256':digest(raw)},'legacy_inputs':len(doc['legacy_inputs']),
            'plan_inputs_including_checksum_file':len(doc['plan_inputs']),'input_results':entries,'checks':checks,
            'failures':failures,'g0_status':'blocked','product_scenarios_verified':0}
    write_json(out/'result.json',result)
    print(json.dumps({k:v for k,v in result.items() if k not in {'input_results','checks'}},ensure_ascii=False,indent=2))
    return 1 if failures else 0

if __name__=='__main__': sys.exit(main())
