"""Freeze codec kit only; no native runtime readiness or end-to-end claim."""
import hashlib,json,pathlib,shutil
ROOT=pathlib.Path(__file__).resolve().parents[1];BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001';SRC=ROOT/'contracts/experimental/agent_host_v3_http_stream'
RUN=BASE/'wire-20260928T212140875847Z/result.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
data=json.loads(RUN.read_text());assert data['status']=='codec_tests_and_vectors_passed_runtime_not_run'
for p,h in data['before'].items():
    if p.endswith('/README.md'):continue
    assert sha(ROOT/p)==h,p
kit=BASE/'wire-kit-001';assert not kit.exists();kit.mkdir();shutil.copytree(SRC,kit/'source');shutil.copytree(RUN.parent/'vectors',kit/'vectors')
(kit/'generated').mkdir();binding=ROOT/data['binding']['path'];assert sha(binding)==data['binding']['sha256'];shutil.copy2(binding,kit/'generated/native_http_capnp.rs')
shutil.copy2(RUN,kit/'producer-result.json')
for p in RUN.parent.glob('*.stdout'):shutil.copy2(p,kit/p.name)
for p in RUN.parent.glob('*.stderr'):shutil.copy2(p,kit/p.name)
manifest={'kind':'schema/codec kit, runtime not ready','major':3,'revision':1,'schema_sha256':sha(SRC/'native_http.capnp'),'generated_rust_sha256':sha(binding),'generator':data['generator'],'capnp_cli_sha256':sha(pathlib.Path(r'C:\Users\Administrator\capnp-bin\capnp.exe')),'source_files':{str(p.relative_to(SRC)).replace('\\','/'):sha(p) for p in sorted(SRC.rglob('*')) if p.is_file()},'files':{str(p.relative_to(kit)).replace('\\','/'):sha(p) for p in sorted(kit.rglob('*')) if p.is_file()},'tests_passed':4,'standalone_valid_vectors':24,'invalid_vectors':2,'original_inputs_unchanged':data['frozen_count'],'note':'README terminal-poll rule added after last tests; no compiled source changed. Vectors are independent samples, not one execution transcript.'}
(kit/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
handoff={'status':'wire_codec_candidate_ready_for_consumer_review_runtime_not_ready','canonical_source':str(SRC.relative_to(ROOT)).replace('\\','/'),'crate':'morrow-native-http-stream-wire','major':3,'revision':1,'schema_sha256':manifest['schema_sha256'],'generated_rust_sha256':manifest['generated_rust_sha256'],'manifest':str((kit/'manifest.json').relative_to(ROOT)).replace('\\','/'),'manifest_sha256':sha(kit/'manifest.json'),'producer_receipt_sha256':sha(RUN),'validation':'4 codec tests,24 standalone valid +2 invalid vectors, offline generator/compiler only','not_run':['native grant adapter','authenticated data pipe / OS pending','host/plugin request approval/dispatch/credit integration','real Core HTTP success','independent codec review']}
(BASE/'wire-handoff-001.json').write_text(json.dumps(handoff,indent=2)+'\n')
print(json.dumps({**handoff,'handoff_sha256':sha(BASE/'wire-handoff-001.json')}))
