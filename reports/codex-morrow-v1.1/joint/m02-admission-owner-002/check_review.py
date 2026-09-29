"""Read-only preflight. Exit 0 integrity-only, 1 mismatch, 2 runtime pending."""
import argparse,hashlib,json,re,subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
JOINT=HERE.parent
HOST=JOINT.parents[2]
def sha(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def read(p):return json.loads(Path(p).read_text(encoding='utf-8-sig'))
def write(p,v):
    p=Path(p).resolve()
    if not p.is_relative_to(HERE) or p.exists():raise ValueError('new batch output only; no overwrite')
    p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def verify(pins):
    rows=[];seen=set()
    for x in pins:
        p=Path(x['path']);expected=x['sha256'];key=str(p.resolve()).casefold()
        if not p.is_absolute() or not re.fullmatch('[0-9a-f]{64}',expected) or key in seen:raise ValueError('invalid/duplicate pin')
        seen.add(key)
        try:actual=sha(p)
        except OSError:actual=None
        rows.append({'path':str(p),'expected':expected,'actual':actual,'match':actual==expected})
    return rows
def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--run-id',required=True);ap.add_argument('--integrity-only',action='store_true');a=ap.parse_args()
    if not re.fullmatch('[a-zA-Z0-9_-]{1,80}',a.run_id):raise ValueError('unsafe run id')
    run=HERE/'runs'/a.run_id;run.mkdir(parents=True,exist_ok=False)
    baseline=read(HERE/'baseline.json');matrix=read(HERE/'matrix.json');index=read(JOINT/'index.json')
    rows=verify(baseline['files']);issues=[x['path'] for x in rows if not x['match']]
    if len(index['acceptance'])!=84 or any(x['status']!='not_run' for x in index['acceptance']):issues.append('original product catalogue changed')
    originals={x['id']:x for x in index['acceptance']}
    for x in matrix['original_acceptance']:
        if originals.get(x['id'])!=x:issues.append('original anchor changed:'+x['id'])
        source=x['source'];lines=Path(source['path']).read_text(encoding='utf-8-sig').splitlines()
        if lines[source['line']-1]!=source['text']:issues.append('source line changed:'+x['id'])
    if len(matrix['diagnostics'])!=14 or {x['id'] for x in matrix['diagnostics']}!={f'M02A-{i:02}' for i in range(1,15)}:issues.append('diagnostic inventory')
    if any(x['status']!='not_run' or x['product_pass_credit']!=0 for x in matrix['diagnostics']):issues.append('preflight cannot confer credit')
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=HOST,text=True).strip();branch=subprocess.check_output(['git','branch','--show-current'],cwd=HOST,text=True).strip()
    if head!='88557916aabf2e10619b1022110035178498898a' or branch!='codex/io-safety-refactor':issues.append('workspace changed')
    code=1 if issues else 0 if a.integrity_only else 2
    result={'scope':'identity_and_preparation_only','status':'integrity_verified_runtime_pending' if not issues else 'integrity_failed','issues':issues,'exit_code':code,'baseline_files':len(rows),'matrix_sha256':sha(HERE/'matrix.json'),'baseline_sha256':sha(HERE/'baseline.json'),'checker_sha256':sha(__file__),'head':head,'branch':branch,'independent_new_runtime_cases':0,'product_pass_credit':0,'M02_complete':False,'G0_passed':False}
    write(run/'identities.json',rows);write(run/'result.json',result);print(json.dumps(result,indent=2));return code
if __name__=='__main__':raise SystemExit(main())
