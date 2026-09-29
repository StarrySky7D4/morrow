"""Seal the reviewed producer transport output; old inputs remain read-only."""
import difflib,hashlib,json,pathlib,shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
SRC=ROOT/'network_node_stream_001';BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
RUN=BASE/'transport-20260928T210834849150Z/result.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
data=json.loads(RUN.read_text());assert data['status']=='transport_producer_tests_passed'
for path,h in data['before'].items():assert sha(ROOT/path)==h,path
for path,h in data['old_before'].items():assert sha(ROOT/path)==h,path
assert data['before']==data['after'] and data['old_before']==data['old_after']
patch=[]
for origin,dest in [('src/client.rs','src/client.rs'),('src/lib.rs','src/lib.rs'),('Cargo.toml','Cargo.toml'),('tests/client.rs','tests/client.rs')]:
    patch.extend(difflib.unified_diff((ROOT/'network_node'/origin).read_text().splitlines(True),(SRC/dest).read_text().splitlines(True),fromfile='network_node/'+origin,tofile='network_node_stream_001/'+dest))
(SRC/'provenance/extraction.patch').write_text(''.join(patch))
kit=BASE/'transport-kit-001';assert not kit.exists();kit.mkdir()
shutil.copytree(SRC,kit/'source')
(kit/'tests').mkdir()
for path,h in data['test_artifacts'].items():
    source=ROOT/path;assert sha(source)==h;shutil.copy2(source,kit/'tests'/source.name)
shutil.copy2(RUN,kit/'producer-result.json')
for suffix in ['tests.stdout','tests.stderr','fmt.stdout','fmt.stderr']:shutil.copy2(RUN.parent/suffix,kit/suffix)
manifest={'scope':'transport-only producer candidate, no native IPC or independent acceptance','inputs':data['before'],'source_files':{str(p.relative_to(SRC)).replace('\\','/'):sha(p) for p in sorted(SRC.rglob('*')) if p.is_file()},'files':{str(p.relative_to(kit)).replace('\\','/'):sha(p) for p in sorted(kit.rglob('*')) if p.is_file()},'producer_receipt_sha256':sha(RUN),'old_inputs_unchanged':len(data['old_before']),'tests':{'inherited_client':10,'stream':6},'not_run':['native data pipe / actual OS Pending','real Core/SSE','independent joint transport review','TLS runtime','public DNS/network','product gates']}
(kit/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
handoff={'status':'transport_producer_ready_for_review','manifest':str((kit/'manifest.json').relative_to(ROOT)).replace('\\','/'),'manifest_sha256':sha(kit/'manifest.json'),'producer_receipt':str(RUN.relative_to(ROOT)).replace('\\','/'),'producer_receipt_sha256':sha(RUN),'tests_passed':16,'old_inputs_unchanged':len(data['old_before']),'prior_failure':'First run passed 15/16; drop test incorrectly required closed-port Windows connection to finish inside 1s. Fixed test uses a separate100ms deadline to distinguish admitted Timeout/Transport from permit Limit. Prior evidence retained.','limitations':manifest['not_run']}
(BASE/'transport-handoff-001.json').write_text(json.dumps(handoff,indent=2)+'\n')
print(json.dumps({'handoff_sha256':sha(BASE/'transport-handoff-001.json'),'manifest_sha256':sha(kit/'manifest.json'),'producer_receipt_sha256':sha(RUN),'old_inputs_unchanged':len(data['old_before'])}))
