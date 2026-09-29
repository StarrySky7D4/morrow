"""Reject non-Cap'n Proto runtime authority before any client launch.

This is a source-format precondition, never proof of generated codec or IPC.
"""
import argparse
import re
import sys
from pathlib import Path
sys.dont_write_bytecode=True
from check_review import HERE, HOST, sha, write

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--run-id',required=True)
    ap.add_argument('--schema',required=True,type=Path)
    ap.add_argument('--sha256',required=True)
    args=ap.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,90}',args.run_id): ap.error('unsafe run id')
    run=HERE/'runs'/args.run_id
    run.parent.mkdir(exist_ok=True); run.mkdir(exist_ok=False)
    result={'scope':'normative_runtime_contract_format_precondition',
            'runtime_accepted':False,'independent_runtime':False,'product_pass_credit':0,
            'checker_sha256':sha(__file__),'issues':[]}
    try:
        plan=HOST.parents[1]/'Codex_Morrow_Plan_v1.1/Codex_Morrow_Plugin_Project_Plan_v1.1_2026-09-28.md'
        line=plan.read_text(encoding='utf-8-sig').splitlines()[49]
        if not line.startswith('自有跨边界消息使用 Cap’n Proto；'):
            raise ValueError('original architectural source changed')
        result['requirement']={'path':str(plan),'line':50,'text':line,'sha256':sha(plan)}
        schema=args.schema.resolve()
        result['schema']={'path':str(schema),'expected':args.sha256.lower(),'actual':sha(schema)}
        if sha(schema)!=args.sha256.lower(): result['issues'].append('schema_identity_mismatch')
        if not schema.is_relative_to(HOST/'contracts/experimental'):
            result['issues'].append('not_host_owned_authority')
        content=schema.read_text(encoding='utf-8-sig')
        if schema.suffix!='.capnp' or not re.search(r'(?m)^\s*@0x[0-9a-fA-F]{16};',content) or not re.search(r'\b(struct|interface)\s+\w+',content):
            result['issues'].append('non_capnp_normative_runtime_contract')
        result['status']='rejected' if result['issues'] else 'source_format_only_pending_generated_and_runtime_verification'
        result['exit_code']=1 if result['issues'] else 2
        result['pending_for_formal_acceptance']=['compiler_and_generated_binding_identity',
          'actual_encoder_decoder_path_uses_authoritative_capnp', 'framing_and_reader_limits',
          'positive_negative_vectors', 'source_authority_review', 'independent_real_ipc']
    except Exception as exc:
        result.update(status='failed',exit_code=1)
        result['issues'].append({'error':type(exc).__name__,'message':str(exc)})
    write(run/'result.json',result)
    import json
    print(json.dumps(result,ensure_ascii=False,indent=2))
    return result['exit_code']

if __name__=='__main__': raise SystemExit(main())
