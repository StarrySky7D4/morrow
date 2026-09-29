"""Consume only the plugin's explicitly ready handoff, without running its tools."""
import argparse
import hashlib
import json
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path
sys.dont_write_bytecode=True
from check_joint import HERE, HOST, digest, write_json
PLUGIN=HOST.parents[2]/'morrow-codex'
PINS={'codex':'44fe510ce3ee61c8ef623adcbf89b901c73ddd61','cc-switch':'846de29c13ac4d65f164db8c15dd5fd58e29f972'}

def safe(root,relative):
    relative=Path(relative); path=(root/relative).resolve()
    if relative.is_absolute() or '..' in relative.parts or not path.is_relative_to(root.resolve()): raise ValueError('unsafe reference')
    return path

def read_json(path): return json.loads(path.read_text(encoding='utf-8-sig'))

def main():
    p=argparse.ArgumentParser(description=__doc__); p.add_argument('--run-id',required=True); args=p.parse_args()
    out=(HERE/'runs'/args.run_id).resolve()
    if not out.is_relative_to(HERE/'runs') or out.exists(): p.error('use a fresh joint run directory')
    out.mkdir(parents=True); (out/'reviewer-source.py').write_bytes(Path(__file__).read_bytes())
    failures=[]; checks=[]; observed={}; counts={}; results=[]
    def need(ok,why):
        if not ok: raise ValueError(why)
    handoff_path=PLUGIN/'receipts/handoff.json'; raw=handoff_path.read_bytes(); handoff_pin=digest(raw)
    try:
        handoff=json.loads(raw); need(handoff['ready_for_review'] is True,'handoff not ready')
        need(Path(handoff['repository']).resolve()==PLUGIN,'repository mismatch')
        for relative,pin in handoff['input_sha256'].items():
            actual=digest(safe(PLUGIN,relative).read_bytes()); need(actual==pin,'handoff input changed: '+relative); observed[relative]=actual
        vp=safe(PLUGIN,handoff['verification_path']); need(digest(vp.read_bytes())==handoff['verification_sha256'],'verification pin mismatch')
        verification=read_json(vp)
        need(verification['inputs_before']==verification['inputs_after']==observed,'before/after/handoff input mismatch')
        need(verification['inputs_stable'] is True and verification['verification_passed'] is True,'producer verification incomplete')
        need(len(verification['checks'])==13,'expected 43-test receipt plus 12 CLI statuses')
        testcount=0; clicount=0
        for check in verification['checks']:
            need(check['passed'] is True and check['exit_code']==check['expected_exit_code'],'producer check mismatch')
            if check['name']=='entry_tests':
                combined=''
                for field in ['stdout','stderr']:
                    path=safe(vp.parent,check[field]); content=path.read_bytes()
                    observed[str(path.relative_to(PLUGIN)).replace('\\','/')]=digest(content)
                    combined+=content.decode('utf-8-sig',errors='replace')
                need('Ran 43 tests' in combined and '\nOK' in combined and 'skipped=' not in combined,'43 test log summary mismatch')
                testcount=43; continue
            sp=safe(vp.parent,check['status']); status=read_json(sp); clicount+=1
            need(status['entry_sha256']==handoff['input_sha256']['tools/build_plan.py'],'CLI receipt uses another entry source')
            need(status['contract_sha256']==handoff['input_sha256']['tools/build-contract.json'],'CLI contract mismatch')
            need(status['exit_code']==check['exit_code'],'CLI exit mismatch')
            need(status['network_used'] is False and status['locks_updated'] is False and status['old_dist_reused'] is False,'CLI side-effect/old artifact flag mismatch')
            need(status['artifacts']==[] and status['gates']=={'P-00':'not_claimed','G0':'not_claimed'},'entry receipt overclaims product gate')
            expected=None if check['name']=='baseline' else ('source_incomplete' if check['name'] in {'sources','full'} else 'not_implemented')
            need(status.get('error',{}).get('code')==expected,'CLI error semantic mismatch')
            results.append({'name':check['name'],'exit_code':status['exit_code'],'error':expected,'receipt_path':str(sp),'sha256':digest(sp.read_bytes())})
        need(testcount==43 and clicount==12,'test/CLI count mismatch')
        checks.append({'name':'ready_inputs_and_43_plus_12_receipts','status':'verified','frozen_inputs':len(handoff['input_sha256']),'test_count':testcount,'cli_count':clicount,'mode':'producer execution receipts independently read/hashed, not re-executed'})
        source_lock=read_json(PLUGIN/'sources.lock.json'); source_results=[]
        need(source_lock['patches']==[],'unexpected upstream patch')
        for source in source_lock['sources']:
            ident=source['id']; need(source['commit']==PINS[ident] and source['kind']=='partial_snapshot','source pin/qualification mismatch')
            root=safe(PLUGIN,source['path']); mp=safe(PLUGIN,source['files_manifest']['path'])
            need(digest(mp.read_bytes())==source['files_manifest']['sha256'],'source manifest hash mismatch')
            files=read_json(mp)['files']; seen=set()
            for item in files:
                need(item['path'] not in seen,'duplicate source member'); seen.add(item['path'])
                content=safe(root,item['path']).read_bytes()
                blob=hashlib.sha1(b'blob '+str(len(content)).encode()+b'\0'+content).hexdigest()
                need(digest(content)==item['sha256'] and blob==item['git_blob_sha1'],'source byte mismatch: '+item['path'])
            actual={f.relative_to(root).as_posix() for f in root.rglob('*') if f.is_file()}
            need(actual==seen,'unmanifested source snapshot member')
            for license in source['license_files']: need(digest(safe(root,license['path']).read_bytes())==license['sha256'],'license mismatch')
            counts[ident]=len(files); source_results.append({'id':ident,'commit':source['commit'],'kind':source['kind'],'files':len(files),'files_manifest_sha256':source['files_manifest']['sha256']})
        need(counts=={'codex':209,'cc-switch':13}==handoff['source_files_verified'],'222 source count mismatch')
        need(handoff['sources_complete'] is False and verification['upstream_sources_complete'] is False,'partial source qualification lost')
        checks.append({'name':'exact_partial_source_membership_and_dual_hashes','status':'verified','sources':source_results,'limit':'producer-reported fixed blob identity, not independent remote retrieval or complete commit tree'})
        contract=read_json(PLUGIN/'tools/build-contract.json')
        for name in ['native','wasm']:
            graph=contract['graphs'][name]
            need(graph['status']=='not_implemented' and graph['manifest'] is None and graph['lock'] is None,'unexpected graph completion claim')
        need(contract['graphs']['native']['target_dir']!=contract['graphs']['wasm']['target_dir'],'shared target declaration')
        lock=contract['toolchain_lock']; need(digest(safe(PLUGIN,lock['path']).read_bytes())==lock['sha256'],'toolchain lock digest mismatch')
        codex=read_json(PLUGIN/'receipts/upstream-codex-dependency-closure.json')
        cc=read_json(PLUGIN/'receipts/upstream-cc-switch-dependency-closure.json')
        need(codex['commit']==PINS['codex'] and cc['commit']==PINS['cc-switch'],'closure source mismatch')
        need(codex['missing_path_manifests']==[],'missing static path manifests')
        for source_id,relative,n in [('codex','codex-rs/Cargo.lock',1471),('cc-switch','src-tauri/Cargo.lock',750)]:
            lockdata=tomllib.loads((PLUGIN/'upstream/reference'/source_id/relative).read_text(encoding='utf-8'))
            need(len(lockdata['package'])==n,'upstream lock package count mismatch')
        checks.append({'name':'source_closure_reports_and_two_graph_boundaries','status':'verified','product_graphs_implemented':0,'product_graphs_planned':2,'limit':'manifest/static closure only; no feature/target-resolved product graph or source-build qualification'})
        # Freeze check after all reads; no intermediate/stale candidate is accepted.
        for relative,pin in handoff['input_sha256'].items(): need(digest(safe(PLUGIN,relative).read_bytes())==pin,'input changed during read: '+relative)
        need(digest(handoff_path.read_bytes())==handoff_pin,'handoff changed during read')
        need(handoff['P-02']=='not_implemented' and handoff['G0']=='blocked' and handoff['product_acceptance_passed']==0,'gate overclaim')
        checks.append({'name':'inputs_still_frozen_and_unimplemented_gates_retained','status':'verified'})
    except Exception as error:
        failures.append(str(error))
    result={'captured_at':datetime.now(timezone.utc).isoformat(),'status':'failed' if failures else 'verified',
      'scope':'P-00 entry receipt and P-01 partial-source static-audit handoff consistency only; neither full work-package exit nor P-02 execution',
      'handoff':{'path':str(handoff_path),'sha256':handoff_pin},'checks':checks,'observed_inputs':observed,'cli_receipts':results,
      'source_file_counts':counts,'failures':failures,'g0_status':'blocked','product_scenarios_verified':0}
    write_json(out/'result.json',result); print(json.dumps({k:v for k,v in result.items() if k not in {'observed_inputs','cli_receipts'}},ensure_ascii=False,indent=2)); return 1 if failures else 0

if __name__=='__main__': sys.exit(main())
