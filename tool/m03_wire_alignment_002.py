"""Pre-freeze schema edits requested by coordinator; no old contract is touched."""
from pathlib import Path
R=Path(__file__).resolve().parents[1];D=R/'contracts/experimental/agent_host_v3_http_stream'
p=D/'native_http.capnp';s=p.read_text().replace('released @23;', 'requestClosed @23;')
s=s.replace('  bodySha256 @4 :Data;','  bodySha256 @4 :Data;\n  responseLimitBytes @5 :UInt32;')
s=s.replace('  sendBudget @6 :UInt32;','  sendBudget @6 :UInt32;\n  responseLimitBytes @7 :UInt32;')
s=s.replace('  maxChunkBytes @5 :UInt32;','  maxChunkBytes @5 :UInt32;\n  errorConsumedBytes @6 :UInt64;')
s=s.replace('  workerStarted @27 :Bool;','  workerStarted @27 :Bool;\n  errorConsumedBytes @28 :UInt64;\n  requestClosed @29 :Bool;');p.write_text(s)
p=D/'src/lib.rs';s=p.read_text()
# Work per struct impl section after cargo fmt.
for name,next_name,fields in [('Prepare','Chunk',[('response_limit_bytes','u32')]),('Decision','Head',[('response_limit_bytes','u32')]),('Credit','Progress',[('error_consumed_bytes','u64')]),('Progress','Payload',[('error_consumed_bytes','u64'),('request_closed','bool')])]:
    start=s.index('pub struct '+name+' {');end=s.index('pub '+('enum' if next_name=='Payload' else 'struct')+' '+next_name,start)
    section=s[start:end]
    pos=section.index('\n}')
    section=section[:pos]+''.join('\n    pub '+n+': '+t+',' for n,t in fields)+section[pos:]
    pos=section.index('\n    }',section.index('fn write'))
    section=section[:pos]+''.join('\n        b.set_'+n+'(self.'+n+');' for n,t in fields)+section[pos:]
    pos=section.index('\n        })',section.index('fn read'))
    section=section[:pos]+''.join('\n            '+n+': r.get_'+n+'(),' for n,t in fields)+section[pos:]
    s=s[:start]+section+s[end:]
p.write_text(s)
p=D/'src/rules.rs';s=p.read_text().replace('Kind::Released','Kind::RequestClosed')
s=s.replace('.checked_add(c.drain_discarded_bytes)', '.checked_add(c.drain_discarded_bytes)\n                    .and_then(|n|n.checked_add(c.error_consumed_bytes))')
s=s.replace('.checked_add(p.drain_discarded_bytes)', '.checked_add(p.drain_discarded_bytes)\n                    .and_then(|n|n.checked_add(p.error_consumed_bytes))')
s=s.replace('p.body_bytes as usize > MAX_BODY','(p.body_bytes as usize > MAX_BODY || p.response_limit_bytes == 0 || p.response_limit_bytes as usize > MAX_RESPONSE)')
s=s.replace('d.body_bytes as usize > MAX_BODY','(d.body_bytes as usize > MAX_BODY || d.response_limit_bytes == 0 || d.response_limit_bytes as usize > MAX_RESPONSE)')
needle='                if p.worker_joined && !p.worker_started {'
s=s.replace(needle,'''                if (self.kind==Kind::RequestClosed && !p.request_closed) || (p.request_closed && ((p.worker_started&&!p.worker_joined)||!p.connect_reaped||!p.read_reaped||!p.write_reaped||!p.data_closed)) {return Err("request cleanup facts");}
'''+needle)
s=s.replace('        || body.len() != prepare.body_bytes as usize','        || body.len() != prepare.body_bytes as usize\n        || prepare.response_limit_bytes==0 || prepare.response_limit_bytes as usize>MAX_RESPONSE')
s=s.replace('    field(&mut out, body);','    out.extend_from_slice(&prepare.response_limit_bytes.to_le_bytes());\n    field(&mut out, body);');p.write_text(s)
for p in [D/'tests/common/mod.rs',D/'tests/codec.rs']:
    s=p.read_text().replace('Kind::Released','Kind::RequestClosed').replace('("released", Kind::RequestClosed)','("request-closed", Kind::RequestClosed)')
    s=s.replace('worker_started: true,','worker_started: true,\n        error_consumed_bytes: 0,\n        request_closed: false,')
    s=s.replace('body_sha256: digest(b"{}").to_vec(),','body_sha256: digest(b"{}").to_vec(),\n        response_limit_bytes: 65536,')
    # Both Prepare and Decision above get the limit via body_sha256 literal.
    s=s.replace('cancel_discarded_bytes: 0,\n        window_bytes:', 'cancel_discarded_bytes: 0,\n        error_consumed_bytes: 0,\n        window_bytes:')
    s=s.replace('cancel_discarded_bytes: 0,\n            window_bytes:', 'cancel_discarded_bytes: 0,\n            error_consumed_bytes: 0,\n            window_bytes:')
    s=s.replace('p.owner_released = true;', 'p.owner_released = true;')
    # RequestClosed is emitted while child is alive; owner fields remain active.
    s=s.replace('p.owner = OwnerPhase::Released;','p.request_closed = true;')
    s=s.replace('            p.child_exited = true;\n            p.stdout_eof = true;\n            p.stderr_eof = true;\n            p.owner_released = true;','')
    p.write_text(s)
print('pre-freeze response budget / error consumption / request cleanup aligned')
