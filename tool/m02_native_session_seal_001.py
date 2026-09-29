"""Seal current bounded runtime candidate and existing evidence; no builds or children."""
import pathlib,json,hashlib,shutil,subprocess,datetime
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m02-native-session-001'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
build=BASE/'run-20260928T192949834970Z/result.json'
pair=BASE/'pair-20260928T193003821828Z/result.json'
negative=BASE/'negative-cli-20260928T193111555966Z/result.json'
b=read(build)
assert b['status'].startswith('passed') and b['before']==b['after']
for name,digest in b['after'].items():assert sha(ROOT/name)==digest,name
for name,digest in b['artifacts'].items():assert sha(ROOT/name)==digest,name
assert b['qualification_result']['case_count']==18
assert read(pair)['status']=='passed_real_frozen_plugin_host_pair'
assert read(negative)['passed'] and read(negative)['host_exit']==2
wire=BASE/'capnp-kit-001/manifest.json'
assert sha(wire)=='2a0146a8f18b658b906c6a5f2d0334272a794edf43b0b6ac5757d9ba45cb66d9'
for name,digest in read(wire)['files'].items():assert sha(wire.parent/name)==digest,name
old=ROOT/'reports/codex-morrow-v1.1/host/host-kit-003/manifest.json'
assert sha(old)=='5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01'
for folder,rows in [(old.parent,read(old)['files']),(ROOT/'contracts/experimental/agent_host_v1',read(old)['source_files'])]:
    for row in rows:assert sha(folder/row['path'])==row['sha256'],row['path']
tracked=subprocess.check_output(['git','diff','--name-only','HEAD'],cwd=ROOT,text=True).strip();assert not tracked,tracked
kit=BASE/'runtime-kit-001';kit.mkdir()
for name in b['artifacts']:
    src=ROOT/name;dst=kit/'bin'/src.name;dst.parent.mkdir(exist_ok=True);shutil.copyfile(src,dst)
for src in (ROOT/'native_session').rglob('*'):
    if src.is_file():
        dst=kit/'native_session'/src.relative_to(ROOT/'native_session');dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dst)
evidence={}
for folder in [build.parent,pair.parent,negative.parent]:
    for src in folder.rglob('*'):
        if src.is_file():evidence[src.relative_to(ROOT).as_posix()]=sha(src)
for src in [BASE/'DELIVERY.md',ROOT/'native_session/README.md',pathlib.Path(__file__),ROOT/'tool/m02_native_session_006.py',ROOT/'tool/m02_native_session_pair_001.py',ROOT/'tool/m02_native_session_negative_cli_001.py']:
    evidence[src.relative_to(ROOT).as_posix()]=sha(src)
plugin=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex\receipts\m02-native-session-001\candidate-001.json')
assert sha(plugin)=='9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'
p=read(plugin)
for name,digest in p['input_sha256'].items():assert sha(plugin.parents[2]/name)==digest,name
manifest={'candidate':'m02-native-session-001/runtime-kit-001','status':'ready_for_independent_review_bounded_native_control_slice','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'workspace':str(ROOT),'branch':subprocess.check_output(['git','branch','--show-current'],cwd=ROOT,text=True).strip(),'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'tracked_diff_empty':True,'wire_manifest':str(wire),'wire_manifest_sha256':sha(wire),'schema_sha256':read(wire)['schema_sha256'],'wire_major':2,'wire_revision':1,'generator':{'capnp_cli':'1.4.0','capnpc':'0.24.0','capnp':'0.24.1'},'host_executable':str(kit/'bin/morrow-native-session-host.exe'),'host_sha256':sha(kit/'bin/morrow-native-session-host.exe'),'plugin_manifest':str(plugin),'plugin_manifest_sha256':sha(plugin),'plugin_executable':p['exe'],'plugin_sha256':sha(pathlib.Path(p['exe'])),'build_result':str(build),'pair_result':str(pair),'negative_cli_result':str(negative),'build_inputs_sha256':b['after'],'evidence_sha256':evidence,'files':{src.relative_to(kit).as_posix():sha(src) for src in sorted(kit.rglob('*')) if src.is_file()},'verification':{'host_cases':18,'real_plugin_pair_cases':3,'negative_cli_exit':2,'frozen_host003_files':180,'frozen_v1_files':12,'before_after_inputs_identical':True},'limits':['Not full M02 or G0/P02/J00 acceptance','Image hash/load TOCTOU remains','No descendant or handle-delegation containment','No production approval UI','Partial write cancellation not dynamically reached','Operator stdout blocking not qualified','No Linux/device/product proof']}
(kit/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
handoff={'status':manifest['status'],'manifest':str(kit/'manifest.json'),'manifest_sha256':sha(kit/'manifest.json'),'host_executable':manifest['host_executable'],'host_sha256':manifest['host_sha256'],'delivery':str(BASE/'DELIVERY.md'),'delivery_sha256':sha(BASE/'DELIVERY.md')}
(BASE/'runtime-handoff-001.json').write_text(json.dumps(handoff,indent=2)+'\n',encoding='utf-8')
print(json.dumps({**handoff,'handoff_sha256':sha(BASE/'runtime-handoff-001.json')}))
