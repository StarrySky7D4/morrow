"""Adapt new-only owner derivative to one shared Store and v3 supervisor interfaces."""
from pathlib import Path
R=Path(__file__).resolve().parents[1];D=R/'native_session_stream_001'
p=D/'src/lib.rs';s=p.read_text();s=s[:s.index('struct Output {')]
s=s.replace('pub use morrow_native_session_wire as wire;','pub use morrow_native_http_stream_wire as wire;\nmod pipe_driver;\nmod http_authority;\nmod supervisor;')
s=s.replace('    pub request_budget: u64,','    pub request_budget: u64,\n    pub http_origin: String,')
s=s.replace('!(100..=60_000).contains(&spec.ttl_ms)','!(100..=10_000).contains(&spec.ttl_ms)')
s=s.replace('    state: Mutex<Snapshot>,','    state: Mutex<Snapshot>,\n    gate: pipe_driver::Gate,')
s=s.replace('    pub events: Vec<Value>,','    pub events: Vec<Value>,\n    pub http: Value,')
s=s.replace('    Stop(oneshot::Sender<()>),','    Stop(oneshot::Sender<()>),\n    ApproveHttp { proposal_ref: Vec<u8>, expected_hash: Vec<u8>, response_limit: u32, ack: oneshot::Sender<Result<Value>> },')
needle='    pub fn snapshot(&self) -> Snapshot {'
s=s.replace(needle,'''    pub async fn approve_http(&self, proposal_ref: Vec<u8>, expected_hash: Vec<u8>, response_limit:u32)->Result<Value>{
        let (ack,reply)=oneshot::channel();
        self.control.try_send(Control::ApproveHttp{proposal_ref,expected_hash,response_limit,ack}).map_err(|_|"HTTP operator queue unavailable")?;
        tokio::time::timeout(Duration::from_secs(2),reply).await.map_err(|_|"HTTP approval acknowledgement unknown")?.map_err(|_|"supervisor ended")?
    }
'''+needle)
s=s.replace('        let (tx, rx) = oneshot::channel();\n        self.control','        self.shared.gate.lock().map_err(|_|"gate poison")?.revoked=true;\n        let (tx, rx) = oneshot::channel();\n        self.control')
s=s.replace('pub fn launch(&mut self, admission: Admission) -> Result<Session>', 'pub fn launch(&mut self, admission: Admission, parent:http_authority::Parent) -> Result<Session>')
s=s.replace('.arg("--morrow-native-session-v2")','.arg("--morrow-native-http-v3")')
s=s.replace('            created: admission.created,\n            state:', '            created: admission.created,\n            gate:parent.gate.clone(),\n            state:')
s=s.replace('                events: vec![],','                events: vec![],\n                http:json!({"phase":"Idle"}),')
s=s.replace('tokio::spawn(supervise(', 'tokio::spawn(supervisor::run(').replace('            rx,\n        ));','            rx,\n            parent,\n        ));')
p.write_text(s)
p=D/'src/authority.rs';s=p.read_text()
for name in ['pack','unpack','connect','load_grant']:s=s.replace('fn '+name, 'pub(crate) fn '+name)
s=s.replace('b"MRNADM01"','b"MRNADM03"').replace('if version != 1','if version != 3')
s=s.replace('PRAGMA user_version=1;','CREATE TABLE http_approvals(id TEXT PRIMARY KEY,payload BLOB NOT NULL) STRICT; PRAGMA user_version=3;')
s=s.replace('g.capabilities != 1','g.capabilities != 3').replace('capabilities: 1,','capabilities: 3,')
s=s.replace('pin: Option<Store>','pin: Option<std::sync::Arc<std::sync::Mutex<Store>>>')
s=s.replace('self.pin = Some(pin);','self.pin = Some(std::sync::Arc::new(std::sync::Mutex::new(pin)));')
s=s.replace('let session = match self.host.launch(admission) {','''let deadline=admission.created+Duration::from_millis(admission.spec.ttl_ms);
        let parent=crate::http_authority::Parent{root:self.root.clone(),approval:g.clone(),store:self.pin.as_ref().unwrap().clone(),gate:std::sync::Arc::new(std::sync::Mutex::new(crate::pipe_driver::EffectGate{revoked:false,deadline,ordinal:0})),origin:admission.spec.http_origin.clone(),deadline};
        let session = match self.host.launch(admission,parent) {''')
s=s.replace('                request_budget: 32,','                request_budget: 32,\n                http_origin:"http://127.0.0.1:1".into(),')
needle='    pub async fn revoke(&mut self, id: &str) -> Result<Value> {'
s=s.replace(needle,'''    pub async fn approve_http(&self, proposal_ref:Vec<u8>, expected_hash:Vec<u8>,response_limit:u32)->Result<Value>{
        if self.failed{return Err("authority failed closed".into());}
        self.session.as_ref().ok_or("no live session")?.approve_http(proposal_ref,expected_hash,response_limit).await
    }
    pub fn inspect_http(&self)->Result<Value>{Ok(self.session.as_ref().ok_or("no session")?.snapshot().http)}
'''+needle)
p.write_text(s)
p=D/'src/main.rs';s=p.read_text().replace('            request_budget: 64,','            request_budget: 128,\n            http_origin:String::new(),')
s=s.replace('            "--operation" => a.operation = v,','            "--operation" => a.operation = v,\n            "--http-origin" => a.spec.http_origin = v,')
s=s.replace('["action","grant_id"]','["action","grant_id","proposal_ref","expected_hash","response_limit"]')
s=s.replace('              "revoke"=>authority.revoke(id).await,','''              "revoke"=>authority.revoke(id).await,
              "inspect_http"=>authority.inspect_http(),
              "approve_http"=>match (hex32(v["proposal_ref"].as_str().unwrap_or("")),hex32(v["expected_hash"].as_str().unwrap_or("")),v["response_limit"].as_u64().and_then(|n|u32::try_from(n).ok())){(Ok(reference),Ok(hash),Some(limit))=>authority.approve_http(reference,hash,limit).await,_=>Err("HTTP approval fields".into())},''')
s+='''
fn hex32(s:&str)->Result<Vec<u8>>{if s.len()!=64||!s.is_ascii(){return Err("digest format".into());} (0..32).map(|i|u8::from_str_radix(&s[i*2..i*2+2],16).map_err(|_|"digest hex".into())).collect()}
'''
p.write_text(s)
print('new owner adapted; supervisor integration still WIP')
