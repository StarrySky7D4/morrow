"""One-shot fresh qualification copy; no changes to previous inputs."""
from pathlib import Path
import shutil, json, hashlib, os

ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
OLD=ROOT/'upstream/p02-source-batch-001/codex-source'
NEW=ROOT/'upstream/p02-integration-004/codex-work'
PROBE=ROOT/'qualification/p02-integration-004'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return p.read_text(encoding='utf-8')
def write(p,s): p.parent.mkdir(parents=True,exist_ok=True); p.write_text(s,encoding='utf-8',newline='\n')
def replace(p,a,b):
    s=read(p)
    assert s.count(a)==1,(str(p),a,s.count(a))
    write(p,s.replace(a,b))

assert not NEW.exists() and not PROBE.exists()
start=json.loads(read(ROOT/'receipts/p02-core-network-003/start.json'))
previous=ROOT/'receipts/p02-core-network-003/handoff.json'
assert sha(previous)=='97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb'
start['frozen_handoffs']['receipts/p02-core-network-003/handoff.json']={'sha256':sha(previous),'input_count':53}
for path,record in start['frozen_handoffs'].items():
    assert sha(ROOT/path)==record['sha256']
    for name,expected in json.loads(read(ROOT/path))['input_sha256'].items(): assert sha(ROOT/name)==expected,name
shutil.copytree(OLD,NEW,symlinks=True)
manifest=json.loads(read(ROOT/'receipts/p02-source-batch-001/codex-content-manifest.json'))
for row in manifest['files']:
    p=NEW/row['path']; data=os.readlink(p).encode() if row['mode']=='120000' else p.read_bytes()
    assert hashlib.sha256(data).hexdigest()==row['sha256']
start.update(working_copy=str(NEW),batch002_exec_patch_carried_forward=True,preflight_sha256=sha(HERE/'preflight.md'))
write(HERE/'start.json',json.dumps(start,indent=2)+'\n')
# Apply reviewed files; only Cargo/lib overlap, combined explicitly below.
for batch,receipt in [('p02-exec-store-002','patch-after-build-001'),('p02-core-network-003','patch-after-build-001')]:
    for row in json.loads(read(ROOT/f'receipts/{batch}/{receipt}.json'))['changes']:
        if batch=='p02-core-network-003' and row['path'] in ['codex-rs/core/Cargo.toml','codex-rs/core/src/lib.rs']: continue
        shutil.copyfile(ROOT/f'upstream/{batch}/codex-work'/row['path'],NEW/row['path'])
core=NEW/'codex-rs/core'
replace(core/'Cargo.toml','morrow-p02-qualification = []','morrow-p02-qualification = []\nmorrow-p02-network-qualification = []\n# Qualification only: deny selected default backends in this entire build.\nmorrow-p02-restricted-qualification = ["morrow-p02-qualification", "morrow-p02-network-qualification", "codex-thread-store/morrow-p02-restricted-qualification"]')
with (core/'src/lib.rs').open('a',encoding='utf-8') as f: f.write('\npub mod morrow_network;\n#[cfg(feature = "morrow-p02-network-qualification")]\npub mod morrow_network_qualification;\n')
replace(core/'src/exec.rs',') -> Result<ExecToolCallOutput> {\n    let ExecRequest {',') -> Result<ExecToolCallOutput> {\n    if cfg!(feature = "morrow-p02-restricted-qualification") {\n        return Err(CodexErr::InvalidRequest("restricted-qualification: independent exec disabled".into()));\n    }\n    let ExecRequest {')
replace(core/'src/unified_exec/process_manager.rs','    ) -> Result<UnifiedExecProcess, UnifiedExecError> {\n        let inherited_fds', '    ) -> Result<UnifiedExecProcess, UnifiedExecError> {\n        if cfg!(feature = "morrow-p02-restricted-qualification")\n            && !environment.has_injected_capabilities()\n        {\n            return Err(UnifiedExecError::create_process("restricted-qualification: injected exec required".into()));\n        }\n        let inherited_fds')
replace(core/'src/client.rs','        let redirect_policy = if api_provider','        if cfg!(feature = "morrow-p02-restricted-qualification") {\n            return Err(crate::error::CodexErr::InvalidRequest("restricted-qualification: injected HTTP required".into()));\n        }\n        let redirect_policy = if api_provider')
replace(core/'src/client.rs','                None => {\n                    ApiWebSocketResponsesClient::new','                None => {\n                    if cfg!(feature = "morrow-p02-restricted-qualification") {\n                        return Err(ApiError::Transport(TransportError::Build("restricted-qualification: injected WebSocket required".into())));\n                    }\n                    ApiWebSocketResponsesClient::new')
store=NEW/'codex-rs/thread-store'
replace(store/'Cargo.toml','[dependencies]','[features]\nmorrow-p02-restricted-qualification = []\n\n[dependencies]')
replace(store/'src/live_thread.rs','        let metadata_sync = ThreadMetadataSync::for_create(&params).await;','        if cfg!(feature = "morrow-p02-restricted-qualification") {\n            return Err(ThreadStoreError::Unsupported { operation: "restricted_qualification_create_before_git" });\n        }\n        let metadata_sync = ThreadMetadataSync::for_create(&params).await;')
replace(store/'src/live_thread.rs','        let should_load_history = params.history.is_none();','        if cfg!(feature = "morrow-p02-restricted-qualification")\n            && thread_store.as_any().is::<LocalThreadStore>()\n        {\n            return Err(ThreadStoreError::Unsupported { operation: "restricted_qualification_local_resume_before_state_db" });\n        }\n        let should_load_history = params.history.is_none();')
# Refactor existing exec bridge so default-environment tests reach original guard.
p=core/'src/morrow_p02_qualification.rs'
s=read(p); a=s.index('    if !environment.has_injected_capabilities()'); b=s.index('    let request = ExecRequest::new(',a)
s=s[:a]+'    let request = request();\n'+s[b:]
a=s.index('    let request = ExecRequest::new('); b=s.index('    UnifiedExecProcessManager::default()',a)
req=s[a:b].replace('    let request = ExecRequest::new(','    ExecRequest::new(').replace('.map_err(|error| error.to_string())?,','.expect("fixed absolute fixture path"),').rstrip().removesuffix(';')
s=s[:a]+s[b:]+ '\nfn request() -> ExecRequest {\n'+req+'\n}\n'
s+='''
/// Enter the independent Core path; never allow a successful spawn in this build.
pub async fn independent_refusal_probe() -> serde_json::Value {
    let spawned = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = spawned.clone();
    let error = crate::exec::execute_exec_request(request(), None, Some(Box::new(move || {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }))).await.expect_err("independent execution must refuse");
    serde_json::json!({"error":error.to_string(),"after_spawn_calls":spawned.load(std::sync::atomic::Ordering::SeqCst)})
}
'''
write(p,s)
p=core/'src/morrow_network_qualification.rs'
replace(p,'pub async fn run(backend: Arc<dyn ModelNetworkBackend>, scenario: Scenario) -> Value {','pub async fn run(backend: Arc<dyn ModelNetworkBackend>, scenario: Scenario) -> Value {\n    run_with_backend(Some(backend), scenario).await\n}\n\n/// Deliberately omit injection to reach the actual default-selection guards.\npub async fn run_without_backend(scenario: Scenario) -> Value {\n    run_with_backend(None, scenario).await\n}\n\nasync fn run_with_backend(backend: Option<Arc<dyn ModelNetworkBackend>>, scenario: Scenario) -> Value {')
replace(p,'    )\n    .with_network_backend(backend);','    );\n    let client = match backend { Some(backend) => client.with_network_backend(backend), None => client };')
# One probe with three modules and one lock; old module main functions are removed.
PROBE.mkdir(parents=True)
for name,batch in [('exec','p02-exec-probe-002'),('store','p02-store-probe-002'),('network','p02-core-network-003')]:
    s=read(ROOT/f'qualification/{batch}/src/main.rs').split('\nfn main() {')[0]
    s=s.replace('async fn qualify()', 'pub(crate) async fn qualify()').replace('mod deny;','use crate::deny;')
    write(PROBE/f'src/{name}.rs',s+'\n')
shutil.copyfile(ROOT/'qualification/p02-exec-probe-002/src/deny.rs',PROBE/'src/deny.rs')
s=read(ROOT/'qualification/p02-core-network-003/Cargo.toml').replace('p02-core-network','p02-integration').replace('-003','-004').replace('features = ["morrow-p02-network-qualification"]','features = ["morrow-p02-restricted-qualification"]')
deps=''
for name,path in [('codex-exec-server','exec-server'),('codex-file-system','file-system'),('codex-utils-path-uri','utils/path-uri'),('codex-thread-store','thread-store'),('codex-protocol','protocol'),('codex-rollout','rollout'),('codex-state','state'),('codex-utils-absolute-path','utils/absolute-path')]:
    deps+=f'{name} = {{ path = "../../upstream/p02-integration-004/codex-work/codex-rs/{path}" }}\n'
deps+='morrow-agent-host-contract = { path = "../../upstream/p02-exec-store-002/host-kit-003", features = ["qualification"] }\n'
s=s.replace('[patch.crates-io]',deps+'\n[patch.crates-io]')
write(PROBE/'Cargo.toml',s)
shutil.copyfile(OLD/'codex-rs/Cargo.lock',PROBE/'Cargo.lock')
write(ROOT/'out/p02-integration-004/cargo-home/config.toml',read(ROOT/'out/p02-core-network-003/cargo-home/config.toml'))
for name in ['run_probe.py','review_working_patch.py','review_resolved_graph.py']:
    s=read(ROOT/'receipts/p02-core-network-003'/name).replace('p02-core-network-003','p02-integration-004').replace('p02-core-network-probe','p02-integration-probe').replace('passed_limited_core_network_probe','passed_limited_integration_probe')
    if name=='run_probe.py':
        a=s.index('    paths.extend(SOURCE / item'); b=s.index('\n    paths.extend(ROOT',a)
        s=s[:a]+'''    paths.extend(SOURCE / item for item in ("Cargo.toml", "Cargo.lock"))
    paths.extend(ROOT / item for item in json.loads((RECEIPTS / "patched-inputs.json").read_text(encoding="utf-8")))'''+s[b:]
        a=s.index('    return {"handoff_sha256"'); b=s.index('\n\n\n',a)
        s=s[:a]+'''    fourth = ROOT / "receipts/p02-core-network-003/handoff.json"
    if digest(fourth) != "97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb":
        raise RuntimeError("Frozen batch003 handoff changed")
    fourth_bound = json.loads(fourth.read_text(encoding="utf-8"))["input_sha256"]
    fourth_observed = {name:digest(safe(ROOT/name)) for name in fourth_bound}
    return {"files":actual,"network_batch_files":observed,"exec_store_batch_files":third_observed,"core_network_files":fourth_observed,
            "matches_frozen_handoff":actual==expected and observed==bound and third_observed==third_bound and fourth_observed==fourth_bound}'''+s[b:]
    write(HERE/name,s)
print('fresh copy and merged initial patch prepared')
