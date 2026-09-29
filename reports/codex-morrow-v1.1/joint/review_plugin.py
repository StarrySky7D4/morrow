"""Read-only receipt and local source consistency; never executes plugin tools."""
import argparse
import hashlib
import json
import sys
from datetime import datetime, timezone
sys.dont_write_bytecode=True
from check_joint import HERE, HOST, digest, write_json
PLUGIN=HOST.parents[2]/'morrow-codex'

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run-id',required=True)
    p.add_argument('--receipt-dir',default='receipts/build-entry-tests-20260928T191737')
    a=p.parse_args(); out=(HERE/'runs'/a.run_id).resolve()
    if not out.is_relative_to(HERE/'runs') or out.exists(): p.error('use a fresh joint run directory')
    observed=[]; failures=[]; source_matches=[]
    receipt_dir=(PLUGIN/a.receipt_dir).resolve()
    if not receipt_dir.is_relative_to(PLUGIN.resolve()): p.error('receipt must remain inside plugin repository')
    receipt_path=receipt_dir/'result.json'; receipt=json.loads(receipt_path.read_text(encoding='utf-8-sig'))
    for key,rel in [('tool_sha256','tools/build_plan.py'),('contract_sha256','tools/build-contract.json'),('test_sha256','tests/test_build_plan.py')]:
        actual=digest((PLUGIN/rel).read_bytes())
        observed.append({'path':str(PLUGIN/rel),'sha256':actual,'receipt_sha256':receipt[key],'match':actual==receipt[key]})
        if actual!=receipt[key]: failures.append('current input differs from test receipt: '+rel)
    log=(receipt_dir/receipt['output']).read_text(encoding='utf-8-sig')
    if receipt['exit_code']!=0 or 'Ran 43 tests' not in log or '\nOK' not in log: failures.append('test log/receipt summary mismatch')
    for name,project,pin in [('upstream-codex-manifests.json','codex','44fe510ce3ee61c8ef623adcbf89b901c73ddd61'),
                             ('upstream-cc-switch-connector.json','cc-switch','846de29c13ac4d65f164db8c15dd5fd58e29f972'),
                             ('upstream-cc-switch-local-deps.json','cc-switch','846de29c13ac4d65f164db8c15dd5fd58e29f972')]:
        rp=PLUGIN/'receipts'/name; data=rp.read_bytes(); manifest=json.loads(data)
        observed.append({'path':str(rp),'sha256':digest(data)})
        root=PLUGIN/'upstream/reference'/project
        for entry in manifest['files']:
            path=(root/entry['path']).resolve()
            if not path.is_relative_to(root.resolve()): raise ValueError('unsafe source reference')
            content=path.read_bytes(); blob=hashlib.sha1(b'blob '+str(len(content)).encode()+b'\0'+content).hexdigest()
            match=blob==entry['git_blob_sha1'] and '/blob/'+pin+'/' in entry['url']
            if 'sha256' in entry: match=match and digest(content)==entry['sha256']
            source_matches.append({'path':str(path),'sha256':digest(content),'git_blob_sha1':blob,'match_reported_blob_and_fixed_url':match})
            if not match: failures.append('reported source blob mismatch: '+entry['path'])
    tree=PLUGIN/'upstream/codex-git-tree-complete.json'
    tree_bytes=tree.read_bytes()
    try:
        parsed=json.loads(tree_bytes); tree_result={'parse':'verified','truncated':parsed.get('truncated'),'sha256':digest(tree_bytes)}
    except ValueError as ex:
        tree_result={'parse':'failed','error':str(ex),'sha256':digest(tree_bytes),'scope':'observed intermediate file, not accepted final provenance'}
    contract=json.loads((PLUGIN/'tools/build-contract.json').read_text(encoding='utf-8-sig'))
    result={'captured_at':datetime.now(timezone.utc).isoformat(),'receipt':{'path':str(receipt_path),'sha256':digest(receipt_path.read_bytes())},
      'scope':'read-only coherence of plugin-delivered receipt, current source hashes and reported fixed source blobs; tests not independently re-executed',
      'integrity_status':'failed' if failures else 'verified','failures':failures,'observed_inputs':observed,
      'build_entry_test_receipt':{'reported_tests':43,'exit_code':receipt['exit_code'],'output_sha256':digest((receipt_dir/receipt['output']).read_bytes()),'qualification':receipt['qualification']},
      'local_source_matches':source_matches,'local_source_file_count':len(source_matches),
      'source_limit':'Local bytes match producer-reported Git blob IDs and fixed URLs only; no independent remote retrieval, complete archive or transitive/runtime closure proof.',
      'codex_tree_observation':tree_result,'graphs':contract['graphs'],'toolchain_lock':contract['toolchain_lock'],
      'g0_status':'blocked','product_scenarios_verified':0}
    write_json(out/'result.json',result)
    print(json.dumps({k:v for k,v in result.items() if k not in {'local_source_matches','observed_inputs','graphs'}},ensure_ascii=False,indent=2))
    return 1 if failures else 0

if __name__=='__main__': sys.exit(main())
