#!/usr/bin/env python3
"""Read-only plan/implementation inspection; writes only beside this script.

This is an acceptance catalogue, not a host wire schema or product test suite.
Exit 0: catalogue integrity; 1: integrity failure; 2: required G0 still blocked.
"""
import argparse
import copy
import csv
import hashlib
import io
import json
import platform
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
HOST = HERE.parents[2]
PLAN = HOST.parents[1] / 'Codex_Morrow_Plan_v1.1'
BASE = '88557916aabf2e10619b1022110035178498898a'
FILES = ['01_Morrow侧扩充实施路径.md', '02_插件侧编码构建路径.md',
         '03_契约交接与验收.md', 'Codex_Morrow_Plugin_Project_Plan_v1.1_2026-09-28.md']
VALID_STATES = {'not_run', 'blocked', 'verified', 'failed'}
# These are review routing suggestions, not extra requirements attributed to the plan.
BASE_OWNERS = {
 'A01':'M-08 P-03','A02':'M-08 P-03','A03':'M-08 P-03','A04':'M-08 P-03','A05':'M-03 M-08 P-03 P-04','A06':'M-06 P-06',
 'M01':'M-03 P-04 P-05','M02':'P-05','M03':'P-05','M04':'P-05','M05':'P-04 P-05','M06':'M-03 P-04 P-08',
 'C01':'M-05 P-07','C02':'M-05 P-07','C03':'M-05 P-07','C04':'M-05 P-07','C05':'M-05 P-07','C06':'M-01 P-07',
 'B01':'M-06 P-09','B02':'M-02 P-09','B03':'P-10','B04':'M-02 P-09 P-10','B05':'M-02 M-06 P-09','B06':'M-06 P-11',
 'P01':'M-02 M-06','P02':'M-05 P-10','P03':'P-02 P-09','P04':'M-08 P-03 P-09 P-12','P05':'P-04 P-05','P06':'M-09 P-13',
 'R01':'M-06 P-09','R02':'M-03 P-08','R03':'M-04 M-07 P-08','R04':'M-06 P-09','R05':'M-04 M-05 P-08','R06':'M-04 P-08',
 'L01':'M-03 M-04 M-07','L02':'M-03 M-04 P-09','L03':'M-07 P-12','L04':'M-06 P-06 P-10','L05':'M-09 P-14','L06':'M-01 M-10 P-00 P-13',
 'X01':'M-11 P-14','X02':'M-11 P-14','X03':'M-11 P-14','X04':'M-10 M-11 P-14','X05':'M-11 P-14','X06':'M-11 P-14'}
GATE_ROUTE = {
 'G0':'M-00 M-01 P-00 P-01 P-02 J-00',
 'G1':'M-02 M-03 M-04 M-05 M-06 M-07 M-08 P-05 P-06 P-08',
 'G2':'M-03 M-04 M-05 M-07 P-04 P-06 P-07 P-08 P-12 J-01',
 'G3':'M-03 M-08 P-03 P-04 P-05 J-01',
 'G4':'M-02 M-06 P-06 P-09 P-10 P-11 J-02',
 'G5':'M-09 M-10 M-11 P-13 P-14 J-03',
 'G6':'M-10 M-11 P-13 P-14 J-03'}

def digest(data):
    return hashlib.sha256(data).hexdigest()

def write_json(path, obj):
    path = path.resolve()
    if not path.is_relative_to(HERE):
        raise ValueError('output outside joint directory')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

def git(*args):
    return subprocess.run(['git', '-C', str(HOST), *args], check=True, capture_output=True).stdout

def rows(name, pattern):
    found = []
    for n, line in enumerate((PLAN / name).read_text(encoding='utf-8-sig').splitlines(), 1):
        if not line.startswith('|'):
            continue
        cells = [s.strip() for s in line.strip('|').split('|')]
        if re.fullmatch(pattern, cells[0]):
            found.append({'id':cells[0], 'cells':cells, 'source':{'path':str(PLAN/name), 'line':n, 'text':line}})
    return found

def refs(text):
    result = set()
    for prefix, start, endprefix, end in re.findall(r'([MPJ])-(\d{2})[–—]([MPJ])-(\d{2})', text):
        if prefix != endprefix or int(end) < int(start):
            raise ValueError('invalid dependency range: ' + text)
        result.update(f'{prefix}-{n:02d}' for n in range(int(start), int(end)+1))
    result.update(re.findall(r'[MPJ]-\d{2}', text))
    return sorted(result)

def catalogue():
    work = []
    for name, pat in [(FILES[0],r'M-\d{2}'), (FILES[1],r'P-\d{2}')]:
        for row in rows(name, pat):
            c = row['cells']
            work.append({'id':row['id'], 'title':c[1], 'dependencies':refs(c[2]),
                         'dependency_text':c[2], 'dependency_scope':'selected_scope' if '已选范围' in c[2] else 'required',
                         'external_dependency':'插件集成' if '插件集成' in c[2] else None,
                         'exit_criteria':c[3], 'source':row['source']})
    lines = (PLAN/FILES[2]).read_text(encoding='utf-8-sig').splitlines()
    jlines = [(n,l) for n,l in enumerate(lines,1) if l.startswith('J-00：')]
    if len(jlines) != 1:
        raise ValueError('expected exactly one joint package source paragraph')
    n, line = jlines[0]
    for ident, text in re.findall(r'(J-\d{2})：([^。]+)', line):
        work.append({'id':ident, 'title':text, 'dependencies':refs(text),
                     'dependency_text':text, 'gate_dependencies':re.findall(r'G\d',text),
                     'exit_criteria':text, 'source':{'path':str(PLAN/FILES[2]),'line':n,'text':line}})
    for item in work:
        item.update(status='blocked' if item['id'] in GATE_ROUTE['G0'].split() else 'not_run',
                    evidence=[], status_reason='G0交接与实际接缝证据待联合复核' if item['id'] in GATE_ROUTE['G0'].split() else '本轮未执行完整工作包退出验收')
    gates = []
    for row in rows(FILES[2],r'G[0-6] .+'):
        c = row['cells']; gid = c[0][:2]
        gates.append({'id':gid,'title':c[0][3:], 'host_requirement':c[1], 'plugin_requirement':c[2],
                      'exit_criteria':c[3], 'explicit_work_dependencies':refs(c[1]+' '+c[2]),
                      'review_work_dependencies':GATE_ROUTE[gid].split(),
                      'routing_basis':'explicit source references plus reviewer routing; not a new plan gate',
                      'status':'blocked' if gid=='G0' else 'not_run', 'evidence':[],
                      'status_reason':'缺少真实P-02三接缝与双端验收证据' if gid=='G0' else '本轮未执行', 'source':row['source']})
    acceptance = []
    for row in rows(FILES[2], r'(?:[AMCBPRLX]\d{2}|[NSEKQ]X-\d{2})'):
        c = row['cells']; ident = row['id']; base = ident in BASE_OWNERS
        owners = BASE_OWNERS[ident].split() if base else refs(c[2])
        boundary = 'unexecuted_product_scenario'
        if ident in {'A01','A02','A03','A05','X06'}: boundary='real_account_or_service_not_authorized'
        if ident in {'X02','X03','X04','X05'}: boundary='target_platform_not_executed'
        if ident == 'X01': boundary='windows_product_flow_not_executed'
        if ident == 'KX-05': boundary='power_loss_facility_not_executed'
        acceptance.append({'id':ident,'group':'base48' if base else 'added36','scenario':c[1],
                           'must_prove':c[2], 'work_packages':owners,
                           'mapping_basis':'reviewer routing; original table has no owner column' if base else 'explicit source ownership',
                           'review_gates':[g for g,ws in GATE_ROUTE.items() if set(owners)&set(ws.split())],
                           'gate_mapping_basis':'reviewer routing by owner intersection; not explicit scenario-to-gate requirements',
                           'status':'not_run','evidence':[],'status_reason':'未执行该完整场景；清单/静态/fake检查不升级场景资格',
                           'boundary':boundary,'source':row['source']})
    migrations = [{'legacy_id':r['id'], 'work_packages':refs(r['cells'][1]), 'change':r['cells'][2], 'source':r['source']}
                  for r in rows(FILES[2],r'W\d{2}')]
    return {'format_version':1,'kind':'joint_acceptance_catalogue_not_wire_schema',
            'baseline':BASE,'work_packages':work,'gates':gates,'acceptance':acceptance,'legacy_mapping':migrations,
            'source_sha256':{f:digest((PLAN/f).read_bytes()) for f in FILES},
            'evidence_policy':'Only complete current-scope scenario execution can mark acceptance verified. Historical reports, document presence, fake tests and script exit 0 cannot.',
            'platform_boundaries':['Windows local catalogue/static/fake checks only', 'No macOS/Linux/Android/iOS/HarmonyOS/Web product qualification', 'No real account, paid request, actual power loss or production isolation qualification']}

def verify_catalogue(cat):
    issues = []
    def need(ok, msg):
        if not ok: issues.append(msg)
    expected_w = {f'M-{n:02d}' for n in range(12)} | {f'P-{n:02d}' for n in range(15)} | {f'J-{n:02d}' for n in range(4)}
    expected_a = {f'{g}{n:02d}' for g in 'AMCBPRLX' for n in range(1,7)} | {f'{g}X-{n:02d}' for g,k in [('N',6),('S',10),('E',8),('K',8),('Q',4)] for n in range(1,k+1)}
    need(len(cat['work_packages'])==31 and {x['id'] for x in cat['work_packages']}==expected_w, '31 work package IDs mismatch/duplicate')
    need(len(cat['gates'])==7 and {x['id'] for x in cat['gates']}=={f'G{n}' for n in range(7)}, 'G0-G6 mismatch/duplicate')
    need(len(cat['acceptance'])==84 and {x['id'] for x in cat['acceptance']}==expected_a, '84 original acceptance IDs mismatch/duplicate')
    need(len(cat['legacy_mapping'])==28 and {x['legacy_id'] for x in cat['legacy_mapping']}=={f'W{n:02d}' for n in range(1,29)}, 'W01-W28 mapping mismatch')
    canonical=catalogue()
    mutable={'status','evidence','status_reason'}
    for field in ['work_packages','gates','acceptance','legacy_mapping']:
        original={x.get('id',x.get('legacy_id')):x for x in canonical[field]}
        for item in cat[field]:
            ident=item.get('id',item.get('legacy_id'))
            if ident in original:
                need({k:v for k,v in item.items() if k not in mutable}=={k:v for k,v in original[ident].items() if k not in mutable},f'{ident}: derived catalogue fields differ from source/parser')
    for field in ['work_packages','gates','acceptance','legacy_mapping']:
        for item in cat[field]:
            label = item.get('id',item.get('legacy_id'))
            for key in ['dependencies','explicit_work_dependencies','review_work_dependencies','work_packages']:
                need(set(item.get(key,[]))<=expected_w, f'{label}: unknown {key}')
            if 'status' in item:
                need(item['status'] in VALID_STATES, f'{label}: invalid status')
                need(item['status']!='verified' or bool(item['evidence']), f'{label}: verified without evidence')
                for ev in item['evidence']:
                    if not isinstance(ev,dict) or not {'path','sha256','scope'}<=set(ev):
                        issues.append(f'{label}: malformed evidence'); continue
                    ep=Path(ev['path'])
                    need(ep.is_absolute() and ep.is_file(), f'{label}: missing evidence file')
                    if ep.is_file(): need(digest(ep.read_bytes())==ev['sha256'],f'{label}: stale evidence digest')
                    if item['status']=='verified' and field=='acceptance':
                        need(ev['scope']=='full_current_product_scenario', f'{label}: non-product evidence cannot qualify scenario')
            src=item['source']; lines=Path(src['path']).read_text(encoding='utf-8-sig').splitlines()
            need(0<src['line']<=len(lines) and lines[src['line']-1]==src['text'],f'{label}: stale source reference')
    # Compare all original rows with the full plan, not just their number.
    patterns=[(FILES[0],r'M-\d{2}'),(FILES[1],r'P-\d{2}'),(FILES[2],r'G[0-6] .+'),(FILES[2],r'(?:[AMCBPRLX]\d{2}|[NSEKQ]X-\d{2})'),(FILES[2],r'W\d{2}')]
    for name,pattern in patterns:
        a=rows(name,pattern); b=rows(FILES[3],pattern)
        need([r['cells'] for r in a]==[r['cells'] for r in b],f'{name}: full plan row content/order differs for {pattern}')
    jsrc=next(x for x in cat['work_packages'] if x['id']=='J-00')['source']['text']
    need(jsrc in (PLAN/FILES[3]).read_text(encoding='utf-8-sig').splitlines(),'joint package paragraph differs from full plan')
    for file,sha in cat['source_sha256'].items():
        need(digest((PLAN/file).read_bytes())==sha, f'source changed: {file}')
    work_state={x['id']:x['status'] for x in cat['work_packages']}
    for gate in cat['gates']:
        if gate['status']=='verified':
            need(all(work_state.get(w)=='verified' for w in gate['explicit_work_dependencies']),f"{gate['id']}: verified with incomplete explicit dependency")
    # Detect cycles only in explicit package dependencies; joint gate links are integration responsibilities.
    graph={x['id']:x.get('dependencies',[]) for x in cat['work_packages']}
    def visit(node, stack, done):
        if node in stack: raise ValueError('work dependency cycle: '+' -> '.join(stack+[node]))
        if node in done: return
        for dep in graph.get(node,[]): visit(dep,stack+[node],done)
        done.add(node)
    try:
        done=set()
        for node in graph: visit(node,[],done)
    except ValueError as ex: issues.append(str(ex))
    return issues

def self_test(cat):
    cases=[]
    def rejected(name, mutate):
        candidate=copy.deepcopy(cat); mutate(candidate)
        failures=verify_catalogue(candidate)
        cases.append({'name':name,'passed':bool(failures),'observed_issues':failures})
    rejected('missing work package',lambda c:c['work_packages'].pop())
    rejected('duplicate acceptance ID',lambda c:c['acceptance'].__setitem__(1,copy.deepcopy(c['acceptance'][0])))
    rejected('unknown dependency',lambda c:c['work_packages'][0]['dependencies'].append('M-99'))
    rejected('stale source reference',lambda c:c['acceptance'][0]['source'].__setitem__('text','fabricated'))
    rejected('verified without evidence',lambda c:c['acceptance'][0].__setitem__('status','verified'))
    rejected('dependency cycle',lambda c:c['work_packages'][0]['dependencies'].append('M-01'))
    rejected('false G0 closure',lambda c:c['gates'][0].update(status='verified',evidence=[]))
    rejected('stale evidence digest',lambda c:c['acceptance'][0]['evidence'].append({'path':str(HERE/'check_joint.py'),'sha256':'0'*64,'scope':'catalogue'}))
    rejected('catalogue evidence masquerades as product',lambda c:c['acceptance'][0].update(status='verified',evidence=[{'path':str(HERE/'check_joint.py'),'sha256':digest((HERE/'check_joint.py').read_bytes()),'scope':'catalogue'}]))
    rejected('altered scenario semantics',lambda c:c['acceptance'][0].__setitem__('scenario','fabricated scenario'))
    rejected('wrong base or added group',lambda c:c['acceptance'][0].__setitem__('group','base48'))
    rejected('deleted P-02 dependencies',lambda c:next(i for i in c['work_packages'] if i['id']=='P-02').__setitem__('dependencies',[]))
    return cases

def snapshot():
    sums=[]
    for line in (PLAN/'SHA256SUMS.txt').read_text(encoding='utf-8-sig').splitlines():
        expected,name=line.split(None,1); name=name.lstrip('*')
        actual=digest((PLAN/name).read_bytes())
        sums.append({'path':str(PLAN/name),'expected':expected,'actual':actual,'match':actual==expected})
    frozen=[]
    tracked=git('ls-tree','-r','--name-only',BASE,'sdk/rust/contracts').decode().splitlines()
    tracked += ['workbench_host/schemas/host.capnp','sdk/rust/src/protocol.rs','core/src/plugin_package.rs']
    for rel in tracked:
        current=HOST/rel; blob=git('show',f'{BASE}:{rel}')
        frozen.append({'path':rel,'baseline_blob_sha256':digest(blob),'disk_sha256':digest(current.read_bytes()),
                       'exact_disk_matches_blob':current.read_bytes()==blob,
                       'git_normalized_unchanged':git('hash-object','--path='+rel,str(current)).strip()==git('rev-parse',f'{BASE}:{rel}').strip()})
    return {'captured_at':datetime.now(timezone.utc).isoformat(),'platform':platform.platform(),'python':sys.version,
            'workspace':str(HOST),'head':git('rev-parse','HEAD').decode().strip(),'branch':git('branch','--show-current').decode().strip(),
            'worktree_status':git('status','--short').decode('utf-8'), 'plan_files':sums, 'frozen_boundary_files':frozen,
            'baseline_limit':'Frozen files checked for byte/source identity only; old guest runtime compatibility not executed.'}

def csv_bytes(cat):
    f=io.StringIO(newline='')
    w=csv.writer(f); w.writerow(['id','group','scenario','must_prove','work_packages','review_gates','status','evidence','boundary','source','line','mapping_basis','gate_mapping_basis'])
    for i in cat['acceptance']:
        w.writerow([i['id'],i['group'],i['scenario'],i['must_prove'],' '.join(i['work_packages']),' '.join(i['review_gates']),i['status'],json.dumps(i['evidence']),i['boundary'],i['source']['path'],i['source']['line'],i['mapping_basis'],i['gate_mapping_basis']])
    return f.getvalue().encode('utf-8-sig')

def export_csv(cat):
    target=(HERE/'coverage.csv').resolve()
    if not target.is_relative_to(HERE): raise ValueError('CSV output outside joint directory')
    target.write_bytes(csv_bytes(cat))

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('command',choices=['generate','check','self-test'])
    p.add_argument('--run-id',required=True,help='new directory name; never reuses an old receipt')
    p.add_argument('--require-g0',action='store_true')
    a=p.parse_args()
    if not re.fullmatch(r'[a-zA-Z0-9_-]+',a.run_id): p.error('invalid run-id')
    out=HERE/'runs'/a.run_id
    if out.exists(): p.error('run-id already exists; use a new receipt directory')
    cat=catalogue() if a.command=='generate' else json.loads((HERE/'index.json').read_text(encoding='utf-8'))
    issues=verify_catalogue(cat); snap=snapshot()
    cases=self_test(cat) if a.command=='self-test' else []
    if not all(i['passed'] for i in cases): issues.append('negative self-test failed to reject invalid input')
    if snap['head']!=BASE or snap['branch']!='codex/io-safety-refactor': issues.append('host baseline branch or HEAD changed')
    if len(snap['plan_files'])!=6 or not all(i['match'] for i in snap['plan_files']): issues.append('plan SHA256SUMS mismatch')
    if not all(i['git_normalized_unchanged'] for i in snap['frozen_boundary_files']): issues.append('frozen boundary changed')
    if a.command!='generate' and (not (HERE/'coverage.csv').is_file() or (HERE/'coverage.csv').read_bytes()!=csv_bytes(cat)):
        issues.append('coverage.csv missing or differs from index')
    if a.command=='generate' and not issues:
        write_json(HERE/'index.json',cat); export_csv(cat)
    g0=next(i for i in cat['gates'] if i['id']=='G0')
    code=1 if issues else (2 if a.require_g0 and g0['status']!='verified' else 0)
    result={'captured_at':snap['captured_at'],'command':a.command,'checker_sha256':digest(Path(__file__).read_bytes()),
            'counts':{'work_packages':len(cat['work_packages']),'gates':len(cat['gates']),'acceptance':len(cat['acceptance']),
                      'base48':sum(i['group']=='base48' for i in cat['acceptance']),'added36':sum(i['group']=='added36' for i in cat['acceptance'])},
            'integrity_status':'failed' if issues else 'verified','issues':issues,
            'g0_status':g0['status'],'product_scenarios_verified':sum(i['status']=='verified' for i in cat['acceptance']),
            'exit_code':code,'scope':'catalogue/reference/source identity only; not product acceptance', 'snapshot':'snapshot.json','negative_self_tests':cases}
    write_json(out/'snapshot.json',snap); write_json(out/'result.json',result)
    print(json.dumps(result,ensure_ascii=False,indent=2))
    return code

if __name__=='__main__':
    try: sys.exit(main())
    except (OSError,ValueError,KeyError,subprocess.CalledProcessError) as ex:
        print('joint check failed: '+str(ex),file=sys.stderr); sys.exit(1)
