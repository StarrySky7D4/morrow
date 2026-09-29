from pathlib import Path
import json,hashlib,shutil
ROOT=Path(__file__).resolve().parents[2];HERE=Path(__file__).resolve().parent
SRC=ROOT/'upstream/p02-integration-004/codex-work/codex-rs'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
checks=[]
for rel,start,guard,effect in [
 ('core/src/unified_exec/process_manager.rs','pub(crate) async fn open_session_with_prepared_exec_env','restricted-qualification: injected exec required','let inherited_fds'),
 ('core/src/exec.rs','pub(crate) async fn execute_exec_request','restricted-qualification: independent exec disabled','let ExecRequest'),
 ('core/src/client.rs','fn build_api_transport','restricted-qualification: injected HTTP required','let client = create_client_for_route'),
 ('core/src/client.rs','async fn connect_websocket','restricted-qualification: injected WebSocket required','ApiWebSocketResponsesClient::new'),
 ('thread-store/src/live_thread.rs','pub async fn create(','restricted_qualification_create_before_git','ThreadMetadataSync::for_create'),
 ('thread-store/src/live_thread.rs','pub async fn resume(','restricted_qualification_local_resume_before_state_db','local_store.state_db().await')]:
    p=SRC/rel;text=p.read_text(encoding='utf-8');offset=text.index(start);g=text.index(guard,offset);e=text.index(effect,offset);assert g<e
    checks.append({'file':rel,'file_sha256':sha(p),'guard_line':text[:g].count('\n')+1,'effect_line':text[:e].count('\n')+1,'source_order_only':True})
host=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor\reports\codex-morrow-v1.1\host')
records=[]
for name,expected in [('p02-integration-004-preflight-review.md','1b84f357742784084f57b9877dadd4bd100cd8563c25f6eb90476f710ff43974'),('p02-integration-004-preflight-inputs.json','4c9dddb8c0bdfae9c29fab50bbb0870bf6e98053c5d12191070fb328f004ecec')]:
    p=host/name;assert sha(p)==expected
    dest=HERE/('host-'+name);assert not dest.exists();shutil.copyfile(p,dest)
    records.append({'original':str(p),'copy':dest.relative_to(ROOT).as_posix(),'sha256':expected})
result={'status':'guard_source_order_verified','checks':checks,'host_preflight':records,'limits':'No OS tracing. Dynamic guard errors and callback counts live in separate runtime receipt.'}
with (HERE/'guard-order-and-host-inputs.json').open('x',encoding='utf-8') as f:json.dump(result,f,indent=2)
build=read(HERE/'runs/build-20260928T141112Z-cab3c0ded0/result.json');run=read(HERE/'runs/run-20260928T141315Z-6ee6091f3e/result.json')
print(json.dumps({'build_duration':build.get('elapsed_seconds'),'run_duration':run.get('elapsed_seconds'),'exe':build['artifacts'],'runtime_sha256':run['runtime_sha256'],'source_order_checks':checks}))
