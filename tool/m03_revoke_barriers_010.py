"""A/B single-case harness. Preparation is pure; real execution needs separate authorization."""
import copy
import datetime
import hashlib
import http.server
import importlib.util
import json
import os
import pathlib
import queue
import secrets
import socket
import subprocess
import sys
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = ROOT / 'reports/codex-morrow-v1.1/host/m03-stream-001'
PLUGIN = pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
PM = PLUGIN / 'receipts/m03-fixture-001/native-candidate-fixture-001.json'
# Filled only after receipt of a separately frozen fixture, never inferred at execution.
PLUGIN_MANIFEST_SHA = '06a90c8d1adb8f09d13e83e177d55a4728792f2214000954c296728962c5b689'
PLUGIN_EXE_SHA = '515ed83d1b27d45df5d4cdcc18c41244b5b6ac930528452cc45a64fe0ada89ad'
HOSTS = {
    'core-revoke': ('004', 'a1c46e448450f1066177e8f51f655f202757517f1cdb83451e7545ac68bbf6f2', 'c9c043c3104573146da1fe2a12a1f009a0a9b71ceb517f8bb6fffca1a4045224'),
    'data-pending': ('005', '395521127ff99358702e57cc13604808eb885ff740b79e02341bb70465d8cb15', '9b2a2412ce9fe617729757b8be7eddb34a84718ad64aa507f2bfdc9de926d6af'),
}
spec_loader = importlib.util.spec_from_file_location('frozen008', ROOT/'tool/m03_http_matrix_008.py')
frozen = importlib.util.module_from_spec(spec_loader); spec_loader.loader.exec_module(frozen)
sha, dump, stamp = frozen.sha, frozen.dump, frozen.stamp
FROZEN_HELPER_SHA='9deef213e4ea7765395fbaf5a0aede45b7cdfc7ff2d75faca662782fa3af39e1'
assert sha(ROOT/'tool/m03_http_matrix_008.py')==FROZEN_HELPER_SHA, 'frozen helper drift'

def fixture_spec(mode, nonce):
    assert mode in HOSTS and len(nonce)==64 and set(nonce)<=set('0123456789abcdef')
    return dict(version=1, mode=mode, nonce=nonce, request_limit=32768,
        response_limit=65536, consumer_events=8, credit_limit=16384, pipe_buffer=1024,
        max_chunk=1024, min_pending_samples=3, min_sample_interval_ms=25,
        revoke_ack_limit_ms=500, events_file='fixture-events.jsonl', release_file='release-consumer.json')

def verify(mode):
    number, mh, eh=HOSTS[mode]; manifest=BASE/f'native-host-candidate-{number}/manifest.json'
    assert sha(manifest)==mh
    hm=json.loads(manifest.read_text());host=pathlib.Path(hm['executable'])
    assert sha(host)==hm['executable_sha256']==eh
    for rel,digest in hm['source_files'].items(): assert sha(manifest.parent/'source'/rel)==digest,rel
    assert PLUGIN_MANIFEST_SHA and PLUGIN_EXE_SHA, 'fixed fixture not bound yet; no launch allowed'
    assert sha(PM)==PLUGIN_MANIFEST_SHA
    pm=json.loads(PM.read_text());guest=pathlib.Path(pm['executable'])
    assert sha(guest)==pm['executable_sha256']==PLUGIN_EXE_SHA
    for rel,digest in pm['input_sha256'].items(): assert sha(PLUGIN/rel)==digest,rel
    return host,guest

class Markers:
    def __init__(self, directory, spec, digest, identity):
        self.path=directory/spec['events_file'];self.spec=spec;self.digest=digest;self.identity=identity
        self.rows=[]
    def read(self):
        if not self.path.exists(): return self.rows
        raw=self.path.read_bytes();assert len(raw)<=1024*1024,'fixture evidence exceeds bound'
        complete=raw[:raw.rfind(b'\n')+1]
        rows=[json.loads(line) for line in complete.splitlines()]
        assert rows[:len(self.rows)]==self.rows,'evidence rewritten'
        for index,row in enumerate(rows):
            assert row['identity']==self.identity
            assert row['mode']==self.spec['mode'] and row['nonce']==self.spec['nonce'] and row['spec_sha256']==self.digest
            assert isinstance(row['ordinal'],int) and (index==0 or row['ordinal']>rows[index-1]['ordinal'])
        self.rows=rows;return rows
    def one(self,kind):
        matches=[r for r in self.read() if r['kind']==kind]
        assert len(matches)<=1, 'duplicate stage '+kind
        return matches[0] if matches else None

def pending_observations(events, require_cancel=False):
    obs=[e['detail'] for e in events if e['event']=='pipe_owner_observation']
    for issue in [o for o in obs if o['stage']=='write_issue' and o['outcome']['kind']=='io_pending' and o['operation']['body_end']]:
        op=issue['operation'];domain=issue['clock_domain']
        same=[o for o in obs if o['operation'] and o['operation']['id']==op['id'] and o['operation']['issue_ordinal']==op['issue_ordinal']]
        assert all(o['clock_domain']==domain and o['before_ns']<=o['after_ns'] for o in same)
        assert all(all(o['operation'][k]==op[k] for k in ('body_end','frame_offset','requested_bytes','initial_pending')) for o in same)
        assert op['initial_pending'] is True
        samples=[o for o in same if o['stage']=='incomplete_sample']
        if len(samples)!=3: continue
        assert all(o['outcome']=={'kind':'incomplete','win32_error':996} for o in samples)
        assert [o['operation']['incomplete_samples'] for o in samples]==[1,2,3]
        assert all(b['before_ns']-a['after_ns']>=25_000_000 for a,b in zip(samples,samples[1:])), 'sample spacing'
        if require_cancel:
            probes=[o for o in same if o['stage']=='cancel_probe']
            assert len(probes)==1 and probes[0]['outcome']['kind']=='incomplete', 'cancel target not reached: operation already completed/unknown'
            assert probes[0]['before_ns']>=samples[-1]['after_ns']
            reaps=[o for o in same if o['stage']=='write_reaped']
            assert len(reaps)==1 and reaps[0]['after_ns']>=probes[0]['after_ns']
            assert reaps[0]['outcome']['id']==op['id']
            assert reaps[0]['outcome']['error'] in (None,995) and 0<=reaps[0]['outcome']['bytes']<=op['requested_bytes']
            cancellation=[o for o in same if o['stage']=='cancel_requested']
            assert len(cancellation)==1 and cancellation[0]['before_ns']>=probes[0]['after_ns'] and reaps[0]['before_ns']>=cancellation[0]['after_ns']
        return dict(id=op['id'],issue_ordinal=op['issue_ordinal'],clock_domain=domain,samples=samples)
    return None

def join_observed(thread, until):
    while thread.is_alive() and time.monotonic()<until:
        time.sleep(min(.002,max(0,until-time.monotonic())))
    if thread.is_alive(): return False
    thread.join(0)
    return True

def core_suppression_checks(qualified, suppressed, reserved_suppressed, gate, core_events):
    """Marker enqueue order is not the cancellation gate's linearization order.

    A is released by this harness only after gate marker receipt. B can awaken
    on the actual cancellation signal before the router enqueues that marker.
    Host source1/application and guest received/gate evidence remain mandatory
    separate checks in run(); none is inferred from these synthetic timings.
    """
    held=qualified['held'];reserved=qualified['reserved']
    return {
        'A_same_event': suppressed['detail']['event']==held['detail']['event'],
        'A_after_release_precondition_marker': suppressed['ordinal']>gate['ordinal'],
        'A_original_gate_rejected': suppressed['detail']['gate_closed'] is True,
        'B_same_event': reserved_suppressed['detail']['event']==reserved['detail']['event'],
        'B_reserved_before_suppression': reserved_suppressed['ordinal']>reserved['ordinal'],
        'B_original_gate_rejected': reserved_suppressed['detail']['gate_closed'] is True,
        'B_original_permit_released': reserved_suppressed['detail']['permit_released'] is True,
        'targets_absent_from_business_output': all(
            not any(e['kind']=='actual_core_output_text_delta' and e['sha256']==target['sha256'] for e in core_events)
            for target in (held['detail']['event'],reserved['detail']['event'])),
    }

class OwnedServer(http.server.ThreadingHTTPServer):
    daemon_threads=False
    block_on_close=False
    def __init__(self, handler):
        self.owned=[];self.sockets=[];self.lock=threading.Lock()
        super().__init__(('127.0.0.1',0),handler)
        self.serving=threading.Thread(target=self.serve_forever,kwargs={'poll_interval':.01},name='fixture-accept')
        self.serving.start()
    def process_request(self,request,address):
        t=threading.Thread(target=self.process_request_thread,args=(request,address),name='fixture-handler')
        with self.lock:self.sockets.append(request);self.owned.append(t)
        t.start()
    def close_owned(self, until):
        # Stop acceptance before taking the handler inventory; no thread/socket
        # created between the inventory and shutdown may escape the join receipt.
        self.shutdown()
        with self.lock:sockets=list(self.sockets);threads=list(self.owned)
        for s in sockets:
            try:s.shutdown(socket.SHUT_RDWR)
            except OSError:pass
            s.close()
        self.server_close()
        receipts=[]
        for t in [self.serving,*threads]:
            receipts.append({'thread':t.name,'joined':join_observed(t,until)})
        receipts.append({'thread':'original_cleanup_budget','joined':time.monotonic()<=until})
        return receipts

class Host:
    def __init__(self,args,env,run):
        self.rows=[];self.commands=[];self.queue=queue.Queue();self.errors=[]
        self.err=(run/'host.stderr').open('xb')
        self.proc=subprocess.Popen(args,cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.err)
        self.reader=threading.Thread(target=self.read,name='host-control-reader');self.reader.start()
    def read(self):
        try:
            while True:
                raw=self.proc.stdout.readline()
                received=time.monotonic_ns()  # Complete-line arrival BEFORE JSON parsing.
                if not raw:break
                value=json.loads(raw);row={'line_received_ns':received,'parsed_ns':time.monotonic_ns(),'value':value}
                self.rows.append(row);self.queue.put(row)
        except Exception as exc:self.errors.append(repr(exc))
        finally:self.queue.put(None)
    def observations(self):
        return [r['value']['observation'] for r in self.rows if r['value'].get('event')=='host_observation']
    def wait(self,predicate,timeout=2):
        until=time.monotonic()+timeout
        while True:
            row=self.queue.get(timeout=max(.001,until-time.monotonic()))
            assert row is not None, 'host stdout ended before expected reply'
            if predicate(row['value']):return row
            assert row['value'].get('event')!='final', 'host ended early; retained final is primary evidence'
    def command(self,action,**fields):
        assert self.proc.poll() is None and not any(r['value'].get('event')=='final' for r in self.rows)
        command={'action':action,**fields};before=time.monotonic_ns()
        self.proc.stdin.write((json.dumps(command)+'\n').encode());self.proc.stdin.flush()
        sent=time.monotonic_ns();self.commands.append({'before_write_ns':before,'after_flush_ns':sent,'value':command})
        row=self.wait(lambda v:v.get('event')=='operator_result' and v.get('action')==action)
        assert row['value']['ok'],row
        if action=='revoke':
            result=row['value']['result'];assert result['persisted'] is True and result['runtime_applied'] is True and result['application_pending'] is False
            assert result['first_revocation_source']==1 and result['first_revocation_reason']==19
            assert row['line_received_ns']-before<=500_000_000,'revoke ACK exceeds fixed 500ms'
        return row['value']['result'],row

def run(mode):
    host_exe,guest_exe=verify(mode)
    batch_stamp=stamp();run=BASE/f'revoke-010-{mode}-{batch_stamp}';run.mkdir()
    evidence=PLUGIN/'out/m03-host-revoke-010'/batch_stamp;evidence.mkdir(parents=True)
    spec=fixture_spec(mode,secrets.token_hex(32));dump(evidence/'fixture-spec.json',spec);spec_sha=sha(evidence/'fixture-spec.json')
    operation='m03-'+mode+'-'+batch_stamp
    profile=run/'profile';cwd=run/'cwd';profile.mkdir();cwd.mkdir()
    dump(run/'locked-expectations.json',dict(mode=mode,spec=spec,spec_sha256=spec_sha,script_sha256=sha(pathlib.Path(__file__)),host=HOSTS[mode],plugin_manifest_sha256=PLUGIN_MANIFEST_SHA,plugin_exe_sha256=PLUGIN_EXE_SHA,original_ttl_ms=10000,handshake_ms=6000,close_ms=1000,one_post_only=True,automatic_retry=False,qualification='two independent cases; only current case authorized',expected=dict(core_terminal='Cancelled',transport_drain=None,host_exit=0,guest_exit=2,http_status=200,intent='Unknown',http_eof=False,response_material_stored=False,error_code=19,revocation_source=1,close_aggregate='Unknown',independent_control_clean=True,guest_threads_joined=True,fixture_threads_joined=True,owner_released=True,ack_observed_upper_bound_ms=500,core_targets_never_business_delivered=True if mode=='core-revoke' else None,read_issue_count_unchanged=True if mode=='data-pending' else None,cancel_probe_still_incomplete=True if mode=='data-pending' else None)))
    abort=threading.Event();body_gate=threading.Event();server_events=[];approved_body=None
    def event(kind,**detail):server_events.append(dict(kind=kind,at_ns=time.monotonic_ns(),**detail))
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*args):pass
        def do_POST(self):
            try:
                self.connection.settimeout(1)
                n=int(self.headers['Content-Length']);assert 0<=n<=32768
                body=self.rfile.read(n);event('post',body_hex=body.hex(),path=self.path)
                assert len([e for e in server_events if e['kind']=='post'])==1
                assert body==approved_body and self.path=='/v1/responses' and 'Authorization' not in self.headers
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Transfer-Encoding','chunked');self.end_headers()
                until=time.monotonic()+5
                while not body_gate.wait(.005):
                    if abort.is_set() or time.monotonic()>=until:return
                prefix=frozen.sse({'type':'response.created','response':{'id':'resp_revoke'}})
                prefix+=frozen.sse({'type':'response.output_text.delta','delta':'barrier-A'})
                prefix+=frozen.sse({'type':'response.output_text.delta','delta':'barrier-B'})
                if mode=='data-pending':prefix+=b':'+b'x'*(32768-len(prefix)-3)+b'\n\n'
                self.wfile.write(f'{len(prefix):x}\r\n'.encode()+prefix+b'\r\n');self.wfile.flush()
                event('body_prefix_sent',bytes=len(prefix),sha256=hashlib.sha256(prefix).hexdigest())
                # Deliberately withhold HTTP EOF and Completed; cancellation owns closure.
                abort.wait(max(0,until-time.monotonic()))
            except Exception as exc:event('server_error',error=repr(exc))
            finally:self.close_connection=True;event('handler_returned')
    server=OwnedServer(Handler);origin=f'http://127.0.0.1:{server.server_address[1]}'
    env={k:os.environ[k] for k in ('SYSTEMROOT','WINDIR','COMSPEC','LOCALAPPDATA') if k in os.environ}
    host=None;result={'status':'failed','mode':mode,'automatic_retry':False};markers=None
    try:
        init=subprocess.run([str(host_exe),'init','--profile',str(profile),'--slot','revoke-slot'],cwd=ROOT,env=env,capture_output=True,timeout=10)
        dump(run/'init.json',{'exit':init.returncode,'stdout':init.stdout.decode(),'stderr':init.stderr.decode()});assert init.returncode==0
        args=[str(host_exe),'serve','--profile',str(profile),'--client',str(guest_exe),'--sha256',PLUGIN_EXE_SHA,'--work-dir',str(cwd),'--plugin-id','morrow-codex','--role','synthetic-http','--operation',operation,'--http-origin',origin,'--ttl-ms','10000','--handshake-ms','6000','--close-ms','1000']
        for arg in ('--fixture-base',origin+'/v1','--evidence-dir',str(evidence),'--max-chunk','1024','--fixture-spec-sha256',spec_sha):args+=['--client-arg',arg]
        dump(run/'launch.json',{'args':args,'env':env});host=Host(args,env,run)
        host.wait(lambda v:v.get('event')=='proposal',10)
        grant=host.command('approve')[0]['grant_id'];identity=host.command('claim',grant_id=grant)[0]
        until=time.monotonic()+6
        while time.monotonic()<until:
            inspected=host.command('inspect_http')[0]
            if inspected.get('proposal'):break
            time.sleep(.01)
        proposal=inspected['proposal'];approved_body,digest=frozen.request_digest(identity,operation,proposal)
        assert len(approved_body)<=32768
        assert proposal['absolute_target']==origin+'/v1/responses' and proposal['method']=='POST' and proposal['response_limit']==65536
        assert json.loads(approved_body)['stream'] is True and not json.loads(approved_body).get('tools')
        dump(run/'independent-approval.json',dict(identity=identity,operation=operation,proposal=proposal,recomputed_request_sha256=digest))
        # The authority's original grant binds the exact execution configuration.
        snapshot=host.command('inspect',grant_id=grant)[0]
        config=snapshot['grant']['config_sha256']
        marker_identity=dict(session=identity['session'],epoch=identity['epoch'],child_pid=identity['pid'],attempt=1,operation_id_sha256=hashlib.sha256(operation.encode()).hexdigest(),host_execution_config_sha256=config)
        markers=Markers(evidence,spec,spec_sha,marker_identity)
        if mode=='data-pending':
            until=time.monotonic()+1
            while not markers.one('data_read_paused'):
                assert time.monotonic()<until,'no full-frame read pause';time.sleep(.005)
            pause=markers.one('data_read_paused')['detail']
            assert pause['framer_bytes']==0 and pause['framer_expected']==4 and pause['read_operation_present'] is False
            dump(run/'qualified-pause.json',markers.one('data_read_paused'))
        assert not server_events
        host.command('approve_http',proposal_ref=proposal['proposal_ref'],expected_hash=digest,response_limit=65536)
        body_gate.set();until=time.monotonic()+3
        qualified=None
        while time.monotonic()<until:
            if mode=='core-revoke':
                queued=markers.one('core_event_queued');held=markers.one('core_event_held');reserved=markers.one('core_event_reserved')
                if queued and held and reserved:
                    assert queued['detail']['event']==held['detail']['event'] and queued['ordinal']<held['ordinal']
                    assert reserved['detail']['event']!=held['detail']['event']
                    for marker,text,number in ((held,'barrier-A',1),(reserved,'barrier-B',2)):
                        target=marker['detail']['event']
                        assert target['kind']=='OutputTextDelta' and target['delta_ordinal']==number and target['bytes']==len(text.encode()) and target['sha256']==hashlib.sha256(text.encode()).hexdigest()
                    qualified={'queued':queued,'held':held,'reserved':reserved};break
            else:
                host.command('inspect_http')
                qualified=pending_observations(host.observations())
                if qualified:break
            time.sleep(.005)
        assert qualified,'target not reached; no retry';dump(run/'qualified-before-revoke.json',qualified)
        ack,ack_row=host.command('revoke',grant_id=grant);dump(run/'revoke-ack.json',{'ack':ack,'row':ack_row,'command':host.commands[-1]})
        until=time.monotonic()+1
        while True:
            observed=host.observations()
            assert not any(e['event']=='revoke_persistence_unconfirmed' for e in observed)
            applied=[e for e in observed if e['event']=='http_cancel_applied' and e['detail']['source']==1 and e['detail']['code']==19 and e['detail']['progress']['revoke_persisted'] is True and e['detail']['progress']['revoke_applied'] is True]
            received=markers.one('host_revoke_received');gate=markers.one('gate_closed')
            if applied and received and gate:
                assert gate['ordinal']>received['ordinal']
                dump(run/'release-preconditions.json',{'harness_observed_ns':time.monotonic_ns(),'host_applied':applied,'guest_revoke':received,'guest_gate':gate})
                break
            assert time.monotonic()<until,'host applied/guest revoke/gate not observed';time.sleep(.005)
        if mode=='core-revoke':
            release=dict(version=1,mode=mode,nonce=spec['nonce'],spec_sha256=spec_sha,identity=marker_identity,stage='release-consumer',event=qualified['held']['detail']['event'])
            temporary=evidence/'release-consumer.pending';dump(temporary,release)
            assert temporary.stat().st_size<=2048
            # Same-directory atomic rename; never overwrite a prior release.
            temporary.rename(evidence/spec['release_file'])
        final=host.wait(lambda v:v.get('event')=='final',3)['value'];host.proc.wait(timeout=1)
        guest=json.loads((evidence/'result.json').read_text());events=final['snapshot']['events'];p=final['snapshot']['http']['progress']
        assert host.proc.returncode==0 and final['snapshot']['exit_observed'] and final['snapshot']['stdout_eof'] and final['snapshot']['stderr_eof']
        assert final['snapshot']['exit_code']==2 and guest['task']['core_terminal']=='Cancelled'
        assert guest['task']['transport_drain'] is None and guest['product_success_claimed'] is False
        assert final['state']['owner']['phase']=='Released' and final['state']['owner']['owner_retained'] is False
        assert p['intent']=='Unknown' and not p['http_eof'] and not p['response_material_stored'] and p['error_code']==19
        assert p['http_status']==200 and 0<=p['peer_consumed_offset']<=p['os_completed_offset']<=p['issued_offset']<=p['reserved_offset']<=p['received_offset']<=65536
        assert p['peer_consumed_offset']==sum(p[k] for k in ('parser_yielded_bytes','drain_discarded_bytes','error_consumed_bytes','cancel_discarded_bytes'))
        assert all(p[k] for k in ('request_closed','worker_joined','connect_reaped','read_reaped','write_reaped','data_closed'))
        assert any(e['event']=='http_cancel_applied' and e['detail']['source']==1 and e['detail']['code']==19 for e in events)
        assert not any(e['event']=='revoke_persistence_unconfirmed' for e in events)
        # Preserve existing negative aggregate semantics while independently proving control clean.
        frozen.PLUGIN_SHA=PLUGIN_EXE_SHA
        checks=frozen.control_checks('s503',final,guest,operation);assert all(checks.values()),checks
        assert guest['local_data_thread_joined'] and not guest['frames_overflow'] and not guest['io_overflow'] and not guest['task']['audit']['overflow'] and not final['snapshot']['event_overflow']
        observation=guest['fixture_observation']
        assert observation['mode']==mode and observation['spec_sha256']==spec_sha and observation['nonce']==spec['nonce']
        assert observation['failure'] is None and observation['evidence_complete'] is True and observation['stages_complete'] is True and observation['gate_closed'] is True
        assert guest['fixture_writer_joined'] is True and observation['writer_joined'] is True and guest['control_threads_joined'] is True
        assert len(guest['control_threads'])==3 and all(t['joined'] and t['finished'] and not t['handle_retained'] for t in guest['control_threads'])
        marker_rows=markers.read();assert len(marker_rows)==observation['written_records']==observation['enqueued_records']
        core_events=[json.loads(line)['event'] for line in (evidence/'core-events.jsonl').read_text().splitlines()]
        if mode=='core-revoke':
            suppressed=markers.one('core_event_suppressed');reserved_suppressed=markers.one('core_reserved_suppressed')
            core_checks=core_suppression_checks(qualified,suppressed,reserved_suppressed,markers.one('gate_closed'),core_events)
            assert all(core_checks.values()),core_checks
            result['core_suppression_checks']=core_checks
            result['B_suppression_marker_preceded_gate_marker']=reserved_suppressed['ordinal']<markers.one('gate_closed')['ordinal']
        else:
            terminal=pending_observations(events,True);assert terminal and terminal['id']==qualified['id'] and terminal['issue_ordinal']==qualified['issue_ordinal']
            assert p['peer_consumed_offset']==0 and p['parser_yielded_bytes']==0
            assert observation['read_issue_count']==pause['read_issue_count']
            reads=[e for e in guest['io'] if e['kind']=='Read' and e['action']=='os_issue']
            assert len(reads)==pause['read_issue_count'] and reads[-1]['id']==pause['last_read_id']
            assert any(e['action']=='os_completion' and e['kind']=='Read' and e['id']==pause['last_read_id'] and e['error'] is None for e in guest['io'])
            assert not any(e['kind']=='actual_core_output_text_delta' for e in core_events)
        verify(mode)
        result.update(status='passed',control_checks=checks,core_terminal=guest['task']['core_terminal'],guest_exit=final['snapshot']['exit_code'],progress=p,guest_result=guest,final=final)
    except Exception as exc:result['error']=repr(exc)
    finally:
        abort.set();until=time.monotonic()+1
        if host:
            if host.proc.poll() is None:
                try:
                    host.commands.append({'before_write_ns':time.monotonic_ns(),'value':{'action':'stop'},'reason':'failure_cleanup_only'})
                    host.proc.stdin.write(b'{"action":"stop"}\n');host.proc.stdin.flush()
                except OSError:pass
                try:host.proc.wait(timeout=max(.001,until-time.monotonic()))
                except subprocess.TimeoutExpired:result['host_cleanup_unconfirmed']=True
            if host.proc.poll() is not None:host.proc.stdin.close()
            result['host_reader_joined']=join_observed(host.reader,until);host.err.close()
            result['host_exit']=host.proc.poll();result['reader_errors']=host.errors
            dump(run/'host-rows.json',host.rows);dump(run/'operator-commands.json',host.commands)
        result['fixture_threads']=server.close_owned(until)
        result['post_count']=sum(e['kind']=='post' for e in server_events)
        if result['post_count']!=1 or any(e['kind']=='server_error' for e in server_events) or not all(t['joined'] for t in result['fixture_threads']) or not result.get('host_reader_joined') or result.get('host_cleanup_unconfirmed') or result.get('reader_errors'):result['status']='failed'
        dump(run/'server-events.json',server_events);dump(run/'result.json',result)
        dump(run/'evidence-manifest.json',{'files':{p.relative_to(run).as_posix():sha(p) for p in sorted(run.rglob('*')) if p.is_file()},'plugin_files':{str(p):sha(p) for p in sorted(evidence.rglob('*')) if p.is_file()}})
    print(json.dumps({'run':str(run),'status':result['status'],'error':result.get('error')}))
    return 0 if result['status']=='passed' else 1

def pure_check():
    for mode in HOSTS:
        s=fixture_spec(mode,'a'*64);assert s['credit_limit']==16384 and s['min_sample_interval_ms']==25
    op=dict(id=7,issue_ordinal=2,body_end=8192,frame_offset=0,requested_bytes=8192,initial_pending=True,incomplete_samples=0)
    def o(stage,before,outcome,count=0):return dict(stage=stage,before_ns=before,after_ns=before+1,operation={**op,'incomplete_samples':count},clock_domain='synthetic',outcome=outcome)
    obs=[o('write_issue',0,{'kind':'io_pending'})]+[o('incomplete_sample',i*26_000_000,{'kind':'incomplete','win32_error':996},i) for i in (1,2,3)]+[o('cancel_probe',80_000_000,{'kind':'incomplete'},3),o('cancel_requested',80_000_002,{'completion_claimed':False},3),o('write_reaped',81_000_000,{'kind':'completed','id':7,'error':995,'bytes':0},3)]
    events=lambda rows:[{'event':'pipe_owner_observation','detail':r} for r in rows]
    assert pending_observations(events(obs),True)
    changed=copy.deepcopy(obs);changed[-3]['outcome']['kind']='completed'
    try:pending_observations(events(changed),True)
    except AssertionError:pass
    else:raise AssertionError('completed cancellation probe accepted')
    assert pending_observations(events(obs[:3])) is None
    changed=copy.deepcopy(obs);changed[2]['before_ns']=changed[1]['after_ns']+24_999_999
    try:pending_observations(events(changed))
    except AssertionError:pass
    else:raise AssertionError('short interval accepted')
    changed=copy.deepcopy(obs);changed[-1]['operation']['issue_ordinal']=3
    try:pending_observations(events(changed),True)
    except AssertionError:pass
    else:raise AssertionError('reap from another issue accepted')
    # File-only synthetic marker tests: no socket/process/real fixture.
    directory=BASE/('barriers-010-pure-markers-'+stamp());directory.mkdir()
    s=fixture_spec('core-revoke','a'*64);identity={'synthetic':True}
    good={'identity':identity,'mode':s['mode'],'nonce':s['nonce'],'spec_sha256':'b'*64,'ordinal':1,'kind':'core_event_held','detail':{}}
    for index,(key,bad) in enumerate((('nonce','c'*64),('spec_sha256','c'*64),('mode','data-pending'),('identity',{'synthetic':False}))):
        child=directory/str(index);child.mkdir();row={**good,key:bad};(child/s['events_file']).write_text(json.dumps(row)+'\n')
        try:Markers(child,s,'b'*64,identity).read()
        except AssertionError:pass
        else:raise AssertionError('cross-batch marker accepted '+key)
    child=directory/'valid';child.mkdir();(child/s['events_file']).write_text(json.dumps(good)+'\n')
    assert Markers(child,s,'b'*64,identity).one('core_event_held')==good
    # Legitimate B wakeup precedes the asynchronous gate marker, while A is
    # released afterwards. These are synthetic classifier inputs, not runtime.
    a={'sha256':'a'*64};b={'sha256':'b'*64}
    qualified={'held':{'detail':{'event':a},'ordinal':2},'reserved':{'detail':{'event':b},'ordinal':3}}
    suppressed={'detail':{'event':a,'gate_closed':True},'ordinal':8}
    reserved_suppressed={'detail':{'event':b,'gate_closed':True,'permit_released':True},'ordinal':4}
    gate={'ordinal':6}
    assert all(core_suppression_checks(qualified,suppressed,reserved_suppressed,gate,[]).values())
    later=copy.deepcopy(reserved_suppressed);later['ordinal']=7
    assert all(core_suppression_checks(qualified,suppressed,later,gate,[]).values())
    for key,value in (('event',a),('gate_closed',False),('permit_released',False)):
        bad=copy.deepcopy(reserved_suppressed);bad['detail'][key]=value
        assert not all(core_suppression_checks(qualified,suppressed,bad,gate,[]).values())
    bad=copy.deepcopy(reserved_suppressed);bad['ordinal']=2
    assert not all(core_suppression_checks(qualified,suppressed,bad,gate,[]).values())
    bad_a=copy.deepcopy(suppressed);bad_a['ordinal']=5
    assert not all(core_suppression_checks(qualified,bad_a,reserved_suppressed,gate,[]).values())
    assert not all(core_suppression_checks(qualified,suppressed,reserved_suppressed,gate,[{'kind':'actual_core_output_text_delta','sha256':b['sha256']}]).values())
    receipt={'status':'pure_only_no_process_socket_or_http','script_sha256':sha(pathlib.Path(__file__)),'cases':list(HOSTS),'fixed_fixture_ready':bool(PLUGIN_MANIFEST_SHA),'post_count':0,'runtime_tested':False,'checks':['fixed-spec limits','qualified same-operation chain','cancel probe completed rejected','insufficient samples rejected','short sample interval rejected','wrong reap issue rejected','four cross-batch marker mutations rejected','valid marker accepted','B suppression before or after gate marker accepted with original gate/permit proof','six negative core suppression mutations rejected; A order remains strict'],'synthetic_marker_evidence':str(directory)}
    out=BASE/('barriers-010-pure-'+stamp()+'.json');dump(out,receipt);print(json.dumps({'receipt':str(out),**receipt}))

if __name__=='__main__':
    if sys.argv[1:]==['--check']:pure_check()
    elif len(sys.argv)==3 and sys.argv[1]=='--execute-authorized-one-case' and sys.argv[2] in HOSTS:sys.exit(run(sys.argv[2]))
    else:raise SystemExit('Use --check. Each actual case requires separate coordinator authorization.')
