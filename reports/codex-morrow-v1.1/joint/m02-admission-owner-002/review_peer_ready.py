"""Read-only source/build/artifact pin review; does not run the delivered peer."""
import json
from pathlib import Path
from check_review import HERE,HOST,sha,read,write,verify
PLUGIN=HOST.parents[2]/'morrow-codex'
def main():
    run=HERE/'runs/peer-ready-001';run.mkdir(parents=True,exist_ok=False)
    p=PLUGIN/'receipts/m02-admission-owner-002/candidate-001.json'
    if sha(p)!='8851b4b05d3f1fe5ffec21bcfe8eeaf67f35b55573e9e95a12b45a25f8e396b5':raise ValueError('peer manifest identity')
    m=read(p);pins={str(PLUGIN/k):v for k,v in m['input_sha256'].items()}
    for k,v in m['frozen_previous_inputs'].items():
        if k in pins and pins[k]!=v:raise ValueError('conflicting prior input')
        pins[k]=v
    pins[str(p)]=sha(p)
    rows=verify([{'path':p,'sha256':s}for p,s in pins.items()]);write(run/'identities.json',rows)
    if not all(x['match'] for x in rows):raise ValueError('changed peer input')
    b=read(m['build']);command=b['commands'][0]
    if command['exit_code']!=0 or b['inputs_before']!=b['inputs_after']:raise ValueError('build state')
    for key in ['stdout','stderr']:
        if sha(Path(m['build']).parent/('0.'+key))!=command[key+'_sha256']:raise ValueError('build log hash')
    messages=[json.loads(x) for x in (Path(m['build']).parent/'0.stdout').read_text().splitlines() if x.startswith('{')]
    artifacts=[x for x in messages if x.get('reason')=='compiler-artifact']
    exe=[x for x in artifacts if x.get('executable')==b['artifact']['path']]
    if len(exe)!=1 or Path(exe[0]['target']['src_path']).resolve()!=PLUGIN/'native/m02-admission-owner-002/src/main.rs':raise ValueError('actual compiled source mismatch')
    if sha(b['artifact']['path'])!=m['exe_sha256'] or sha(m['exe'])!=m['exe_sha256']:raise ValueError('exe differs from build')
    if not any('capnp-kit-001/wire#' in x['package_id'].replace('\\','/') for x in artifacts):raise ValueError('actual host Capnp dependency missing')
    result={'status':'verified_read_only_ready_peer','manifest':str(p),'manifest_sha256':sha(p),'peer_executable':m['exe'],'peer_sha256':m['exe_sha256'],'input_count':len(m['input_sha256']),'previous_input_count':len(m['frozen_previous_inputs']),'deduplicated_pins':len(rows),'actual_compiler_package_ids':sorted({x['package_id'] for x in artifacts}),'source_audit':'capture chain, exact historical send_bytes, bounded observation delay, same exe hidden holder with explicit PID record, no approval API','ACL_tool_scope':'plugin fixed tool reviewed but not executed by joint; new joint-scoped equivalent required','independent_build':False,'independent_runtime':False,'product_pass_credit':0,'reviewer_sha256':sha(__file__)}
    write(run/'result.json',result);print(json.dumps({k:v for k,v in result.items() if k!='actual_compiler_package_ids'},indent=2))
if __name__=='__main__':main()
