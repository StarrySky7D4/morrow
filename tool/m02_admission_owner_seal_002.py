"""Seal actual new candidate; verify compiled source, old freezes and real receipts."""
import pathlib,json,hashlib,shutil,subprocess,datetime
ROOT=pathlib.Path(__file__).resolve().parents[1];BASE=ROOT/'reports/codex-morrow-v1.1/host/m02-admission-owner-002';OLD=ROOT/'reports/codex-morrow-v1.1/host/m02-native-session-001'
PLUGIN=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
build=BASE/'run-20260928T202924609447Z/result.json';process=BASE/'process-20260928T202950668865Z/result.json';peer=BASE/'peer-20260928T203125991615Z/result.json'
b=read(build);assert b['status'].startswith('built_api_tests_passed')and b['before']==b['after']
for name,digest in b['after'].items():assert sha(ROOT/name)==digest,name
host=ROOT/'target/m02-admission-owner-002/debug/morrow-native-owner-host.exe'
assert sha(host)==b['artifacts'][host.relative_to(ROOT).as_posix()]
for p,count in [(process,12),(peer,4)]:
 r=read(p);assert r['status'].startswith('passed')and len(r['cases'])==count and all(x['passed']for x in r['cases'])and r['host_sha256']==sha(host)
old=read(OLD/'runtime-kit-001/manifest.json');old_paths={**old['build_inputs_sha256'],**old['evidence_sha256']}
for name,digest in old_paths.items():assert sha(ROOT/name)==digest,name
for name,digest in old['files'].items():assert sha(OLD/'runtime-kit-001'/name)==digest,name
joint=ROOT/'reports/codex-morrow-v1.1/joint/m02-native-session-001'
assert sha(joint/'ready-handoff.json')=='263be0729c12ff25ac68c5b37b864d0b4ec44eacf7ee66a80cc6f1a4df2e8418'
for row in read(joint/'evidence-manifest.json')['files']:assert sha(pathlib.Path(row['path']))==row['sha256'],row['path']
wire=OLD/'capnp-kit-001/manifest.json';assert sha(wire)=='2a0146a8f18b658b906c6a5f2d0334272a794edf43b0b6ac5757d9ba45cb66d9'
for name,digest in read(wire)['files'].items():assert sha(wire.parent/name)==digest,name
plugin=PLUGIN/'receipts/m02-admission-owner-002/candidate-001.json';assert sha(plugin)=='8851b4b05d3f1fe5ffec21bcfe8eeaf67f35b55573e9e95a12b45a25f8e396b5'
for name,digest in read(plugin)['input_sha256'].items():assert sha(PLUGIN/name)==digest,name
assert not subprocess.check_output(['git','diff','--name-only','HEAD'],cwd=ROOT,text=True).strip()
kit=BASE/'runtime-kit-002';kit.mkdir();(kit/'bin').mkdir();shutil.copyfile(host,kit/'bin'/host.name)
for src in (ROOT/'native_session_owner_002').rglob('*'):
 if src.is_file():
  dst=kit/'native_session_owner_002'/src.relative_to(ROOT/'native_session_owner_002');dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dst)
generated=[]
for line in (build.parent/'build.stdout').read_text(encoding='utf-8').splitlines():
 try:r=json.loads(line)
 except json.JSONDecodeError:continue
 if r.get('reason')=='build-script-executed' and 'morrow-native-session-owner'in r.get('package_id',''):
  src=pathlib.Path(r['out_dir'])/'morrow.native_admission.v1.rs'
  if src.is_file():generated.append(src)
assert generated;src=generated[-1];shutil.copyfile(src,kit/src.name)
evidence={}
for folder in [build.parent,process.parent,peer.parent]:
 for src in folder.rglob('*'):
  if src.is_file():evidence[str(src)]=sha(src)
for src in [BASE/'DELIVERY.md',BASE/'IMPLEMENTATION-BOUNDARY-001.md',pathlib.Path(__file__),ROOT/'tool/m02_admission_owner_build_002.py',ROOT/'tool/m02_admission_owner_process_004.py',ROOT/'tool/m02_admission_owner_peer_002.py',ROOT/'tool/m02_owner_private_dir_001.ps1']:
 evidence[str(src)]=sha(src)
material=pathlib.Path(read(peer)['materials']['root'])
for src in material.rglob('*'):
 if src.is_file():evidence[str(src)]=sha(src)
manifest={'status':'ready_for_independent_review_not_M02_complete','candidate':'m02-admission-owner-002/runtime-kit-002','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source':str(ROOT/'native_session_owner_002'),'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'tracked_diff_empty':True,'host_executable':str(kit/'bin'/host.name),'host_sha256':sha(host),'wire_manifest':str(wire),'wire_manifest_sha256':sha(wire),'wire_unchanged':True,'native_persistence_schema':'native_session_owner_002/schemas/native_admission.proto','native_persistence':'generated Protobuf+LZ4, version1 MRNADM01 envelope; no JSON authority','generated_prost_sha256':sha(kit/'morrow.native_admission.v1.rs'),'prost_build':'0.14.4','protoc_bin_vendored':'3.2.0','approval_owner_scope':'one trusted same-user fixed canonical profile, one slot, original LOCALAPPDATA namespace','core_reuse':['Store::pin_service_authority persisted identity OS lock','HostPolicy activation/revocation/stop/retire'],'plugin_peer_manifest':str(plugin),'plugin_peer_manifest_sha256':sha(plugin),'build_inputs_sha256':b['after'],'files':{src.relative_to(kit).as_posix():sha(src)for src in sorted(kit.rglob('*'))if src.is_file()},'evidence_sha256':evidence,'receipts':{'build_api':str(build),'real_process':str(process),'real_historical_peer':str(peer)},'qualification':{'api_tests':3,'approval_field_mutations':11,'real_process_cases':12,'real_peer_cases':4,'independent_review':'pending'},'old_frozen_inputs_verified':{'old_runtime_bound_paths':len(old_paths),'old_runtime_kit':len(old['files']),'old_joint_evidence':len(read(joint/'evidence-manifest.json')['files']),'old_capnp_kit':len(read(wire)['files']),'plugin_peer_inputs':len(read(plugin)['input_sha256'])},'limits':['No production approval UI or authenticated CLI/installed plugin identity','Profile copies and relocation rejected; no global owner across arbitrary profiles','Fail-closed crash retention, not complete recovery','No OS sandbox or image-load TOCTOU closure','No in-flight partial-write/saturation proof','No business or product acceptance credit'],'M-02':'partial','G0':'blocked','P-02':'blocked','J-00':'blocked','G1':'not_passed','product_graphs':'0/2','product_not_run':84}
(kit/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
handoff={'status':manifest['status'],'manifest':str(kit/'manifest.json'),'manifest_sha256':sha(kit/'manifest.json'),'host_executable':manifest['host_executable'],'host_sha256':sha(host),'delivery':str(BASE/'DELIVERY.md'),'delivery_sha256':sha(BASE/'DELIVERY.md')}
(BASE/'ready-handoff-001.json').write_text(json.dumps(handoff,indent=2)+'\n');print(json.dumps({**handoff,'handoff_sha256':sha(BASE/'ready-handoff-001.json')}))

