"""One fresh synthetic loopback POST; independently hash original proposal bytes.
All candidates and evidence are append-only. Never retries a request/profile.
"""
import ctypes, ctypes.wintypes, datetime, hashlib, http.server, json, os, pathlib, queue, shutil, struct, subprocess, sys, threading, time
ROOT=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
BASE=pathlib.Path(__file__).resolve().parent
HOST_BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
PLUGIN=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def dump(p,v): p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def sse(v): return ('data: '+json.dumps(v,separators=(',',':'))+'\n\n').encode()

KERNEL=ctypes.WinDLL('kernel32',use_last_error=True)
KERNEL.OpenProcess.argtypes=[ctypes.wintypes.DWORD,ctypes.wintypes.BOOL,ctypes.wintypes.DWORD];KERNEL.OpenProcess.restype=ctypes.wintypes.HANDLE
KERNEL.WaitForSingleObject.argtypes=[ctypes.wintypes.HANDLE,ctypes.wintypes.DWORD];KERNEL.WaitForSingleObject.restype=ctypes.wintypes.DWORD
KERNEL.GetExitCodeProcess.argtypes=[ctypes.wintypes.HANDLE,ctypes.POINTER(ctypes.wintypes.DWORD)];KERNEL.GetExitCodeProcess.restype=ctypes.wintypes.BOOL
KERNEL.GetProcessTimes.argtypes=[ctypes.wintypes.HANDLE]+[ctypes.POINTER(ctypes.wintypes.FILETIME)]*4;KERNEL.GetProcessTimes.restype=ctypes.wintypes.BOOL
KERNEL.CloseHandle.argtypes=[ctypes.wintypes.HANDLE];KERNEL.CloseHandle.restype=ctypes.wintypes.BOOL
def hold_guest(pid):
    handle=KERNEL.OpenProcess(0x100000|0x1000,False,pid)
    if not handle:raise ctypes.WinError(ctypes.get_last_error())
    times=[ctypes.wintypes.FILETIME() for _ in range(4)]
    if not KERNEL.GetProcessTimes(handle,*[ctypes.byref(t) for t in times]):raise ctypes.WinError(ctypes.get_last_error())
    return handle,{'pid':pid,'creation_filetime':(times[0].dwHighDateTime<<32)|times[0].dwLowDateTime,'initial_wait':int(KERNEL.WaitForSingleObject(handle,0)),'identity_source':'this host claim reply only'}
def observe_guest(handle):
    waited=KERNEL.WaitForSingleObject(handle,5000);code=ctypes.wintypes.DWORD()
    if not KERNEL.GetExitCodeProcess(handle,ctypes.byref(code)):raise ctypes.WinError(ctypes.get_last_error())
    return {'wait_signaled':waited==0,'wait_result':int(waited),'exit_code':code.value}

def main():
    if sys.argv[1:] != ['--authorized-single-joint-run']:raise RuntimeError('one authorized independent host003/plugin002 positive only')
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    run=BASE/('native-pair-'+stamp);run.mkdir()
    evidence=PLUGIN/'out/m03-joint-001'/'positive-independent-001'/stamp;evidence.mkdir(parents=True)
    profile=run/'profile';profile.mkdir();cwd=run/'guest-cwd';cwd.mkdir()
    candidate=run/'candidate';candidate.mkdir();host=candidate/'morrow-native-stream-host.exe'
    shutil.copy2(HOST_BASE/'native-host-candidate-003/morrow-native-stream-host.exe',host)
    host_manifest=HOST_BASE/'native-host-candidate-003/manifest.json'
    assert sha(host_manifest)=='564b58185f5d022fb630fcdfb15367fc358937df23db6a31be1d31ae84e4aa37'
    hm=json.loads(host_manifest.read_text());assert sha(host)==hm['executable_sha256']=='3341dbbe55c271cecfe2c69261374c1623c4ce65c2f96be3bad16b134aff5e65'
    for p,h in hm['source_files'].items():assert sha(host_manifest.parent/'source'/p)==h,p
    inputs={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted((host_manifest.parent/'source').rglob('*')) if p.is_file()}
    for source in ['network_node_stream_001','contracts/experimental/agent_host_v3_http_stream','native_pipe_win_001']:
        inputs.update({str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted((ROOT/source).rglob('*')) if p.is_file() and 'target' not in p.parts})
    manifest=PLUGIN/'receipts/m03-stream-002/native-candidate-002.json'
    assert sha(manifest)=='20135ba780b86a344eeb396fd8d8f0ba85ee3600f79308140380c3fc94fbf561'
    pm=json.loads(manifest.read_text());guest=pathlib.Path(pm['executable'])
    assert sha(guest)==pm['executable_sha256']=='f59665f639f3f6f9157111e4811cccc32569ec47cc63850fe936c13dddd5d32e'
    for p,h in pm['input_sha256'].items(): assert sha(PLUGIN/p)==h,p
    frozen={'host_sha256':sha(host),'host_manifest_sha256':sha(host_manifest),'source_sha256':inputs,'plugin_manifest_sha256':sha(manifest),'plugin_executable_sha256':sha(guest),'harness_sha256':sha(pathlib.Path(__file__)),'harness_revision':'joint positive-independent-001 adapted from pair004; held guest handle, structured delta gate, full byte/closure assertions: explicit handshake 6000ms within original 10000ms; final/exit-aware commands and root cause preservation; fresh grant/profile/op; no retry', 'handshake_ms':6000, 'original_ttl_ms':10000,'no_post_retry':True}
    dump(candidate/'manifest.json',frozen)
    start=time.monotonic_ns();server_rows=[];approved_body=None;request_count=0;request_lock=threading.Lock()
    def mark(event,**kw):server_rows.append({'at_ns':time.monotonic_ns()-start,'event':event,**kw})
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*a):pass
        def do_POST(self):
            nonlocal request_count
            try:
                self.connection.settimeout(3)
                with request_lock:
                    request_count+=1
                    assert request_count==1,'second POST forbidden'
                assert self.headers.get('Content-Length') and 0<int(self.headers['Content-Length'])<=32768
                raw=self.rfile.read(int(self.headers['Content-Length']))
                mark('post_received',path=self.path,body_hex=raw.hex(),headers=list(self.headers.items()))
                assert raw==approved_body,'server bytes differ from independent approval'
                assert self.path=='/v1/responses'
                assert 'Authorization' not in self.headers
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Transfer-Encoding','chunked');self.end_headers()
                def chunk(b):self.wfile.write(('%x\r\n'%len(b)).encode()+b+b'\r\n');self.wfile.flush()
                chunk(sse({'type':'response.created','response':{'id':'resp_m03'}})+sse({'type':'response.output_text.delta','delta':'M03 synthetic delta'}))
                mark('first_sse_written')
                until=time.monotonic()+5
                while time.monotonic()<until:
                    f=evidence/'core-events.jsonl'
                    if f.exists():
                        complete_lines=f.read_text().splitlines()
                        actual=[]
                        for line in complete_lines:
                            try: item=json.loads(line)
                            except json.JSONDecodeError: continue
                            if item.get('event',{}).get('kind')=='actual_core_output_text_delta':actual.append(item)
                        if actual:
                            assert actual[0]['event']['bytes']==19 and actual[0]['event']['sha256']==hashlib.sha256(b'M03 synthetic delta').hexdigest()
                            break
                    time.sleep(.005)
                else:raise RuntimeError('actual Core delta not observed before EOF gate deadline')
                mark('actual_core_delta_observed_before_eof')
                chunk(sse({'type':'response.completed','response':{'id':'resp_m03','usage':{'input_tokens':1,'output_tokens':1,'total_tokens':2}}}))
                # Core completion is followed by a bounded transport-only tail.
                chunk(b': drain tail\n\n');time.sleep(.03)
                self.wfile.write(b'0\r\n\r\n');self.wfile.flush();mark('http_eof_sent')
            except Exception as e:mark('server_error',error=repr(e));self.close_connection=True
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=False
    port=server.server_address[1];server_thread=threading.Thread(target=server.serve_forever);server_thread.start()
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','LOCALAPPDATA'] if k in os.environ}
    result={'status':'failed','run':str(run),'plugin_evidence':str(evidence),'candidate_manifest_sha256':sha(candidate/'manifest.json'),'handshake_ms':6000,'original_ttl_ms':10000,'timing':{}}
    proc=None;errfile=None;rt=None;final=None;rows=[];commands=[];child_identity=None;guest_handle=None
    try:
        init=subprocess.run([str(host),'init','--profile',str(profile),'--slot','m03-slot'],env=env,cwd=ROOT,capture_output=True,timeout=10)
        (run/'init.stdout').write_bytes(init.stdout);(run/'init.stderr').write_bytes(init.stderr);assert init.returncode==0,init.stderr
        operation='m03-joint-positive-'+stamp
        args=[str(host),'serve','--profile',str(profile),'--client',str(guest),'--sha256',sha(guest),'--work-dir',str(cwd),'--plugin-id','morrow-codex','--role','synthetic-http','--operation',operation,'--http-origin',f'http://127.0.0.1:{port}','--ttl-ms','10000','--handshake-ms','6000','--close-ms','1000']
        for v in ['--fixture-base',f'http://127.0.0.1:{port}/v1','--evidence-dir',str(evidence),'--max-chunk','1024']:args+=['--client-arg',v]
        dump(run/'launch.json',{'args':args,'env':env})
        q=queue.Queue();errfile=(run/'host.stderr').open('wb')
        proc=subprocess.Popen(args,env=env,cwd=ROOT,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=errfile,text=True,encoding='utf-8')
        def reader():
            for line in proc.stdout:
                row=json.loads(line);rows.append({'at_ns':time.monotonic_ns()-start,'value':row});q.put(row)
            q.put(None)
        rt=threading.Thread(target=reader,daemon=True);rt.start()
        def retained_final():
            return next((r['value'] for r in reversed(rows) if r['value'].get('event')=='final'),None)
        def stopped_error():
            f=retained_final()
            if f:
                reasons=[e['detail'] for e in f['snapshot']['events'] if e['event'] in ['session_close_reason','request_denied','pipe_driver_error','network_supervision_error']]
                return RuntimeError('host terminal before expected result: '+json.dumps(reasons))
            return RuntimeError('host process exited before expected result; exit='+str(proc.poll()))
        def receive(predicate,timeout=12):
            until=time.monotonic()+timeout
            while time.monotonic()<until:
                v=q.get(timeout=max(.01,until-time.monotonic()))
                if v is None:raise stopped_error()
                if predicate(v):return v
                if v.get('event')=='final':raise stopped_error()
            raise TimeoutError('host result timeout')
        def command(v):
            if retained_final() is not None or proc.poll() is not None:raise stopped_error()
            commands.append({'at_ns':time.monotonic_ns()-start,'value':v})
            try:proc.stdin.write(json.dumps(v)+'\n');proc.stdin.flush()
            except OSError:
                if rt is not None:rt.join(timeout=.2)
                if retained_final() is not None or proc.poll() is not None:raise stopped_error()
                raise
            r=receive(lambda r:r.get('event')=='operator_result' and r.get('action')==v['action'])
            assert r['ok'],r
            return r['result']
        receive(lambda r:r.get('event')=='proposal')
        grant=command({'action':'approve'})['grant_id'];child_identity=command({'action':'claim','grant_id':grant})
        guest_handle=hold_guest(child_identity['pid']);result['held_guest_initial']=guest_handle[1]
        until=time.monotonic()+6
        while time.monotonic()<until:
            h=command({'action':'inspect_http'});p=h.get('proposal')
            if p:
                result['timing']['complete_proposal_observed_at_ns']=time.monotonic_ns()-start
                break
            time.sleep(.015)
        else:raise RuntimeError('no complete HTTP proposal')
        raw=bytes.fromhex(p['body_hex']);assert hashlib.sha256(raw).hexdigest()==p['body_sha256']
        # Independent domain/framing implementation, no host request_digest helper.
        u32=lambda n:struct.pack('<I',n);u64=lambda n:struct.pack('<Q',n)
        field=lambda b:u32(len(b))+b
        pre=b'Morrow/native-http-proposal/v1\0'+u64(child_identity['session'])+u64(child_identity['epoch'])+field(operation.encode())+u64(1)+field(p['method'].encode())+field(p['absolute_target'].encode())+u32(len(p['headers']))
        for header in p['headers']:pre+=field(header['name'].encode())+field(bytes.fromhex(header['value_hex']))
        pre+=u32(p['response_limit'])+field(raw)
        digest=hashlib.sha256(pre).hexdigest();assert digest==p['request_sha256']
        (run/'request-preimage.bin').write_bytes(pre)
        (run/'approved-body.bin').write_bytes(raw)
        assert p['method']=='POST' and p['absolute_target']==f'http://127.0.0.1:{port}/v1/responses'
        body=json.loads(raw);assert body.get('stream') is True and not body.get('tools')
        assert not server_rows,'network activity before trusted approval'
        approved_body=raw;dump(run/'independent-approval.json',{'proposal':p,'body_json':body,'recomputed_request_sha256':digest,'child_identity':child_identity,'operation':operation})
        command({'action':'approve_http','proposal_ref':p['proposal_ref'],'expected_hash':digest,'response_limit':p['response_limit']})
        final=receive(lambda r:r.get('event')=='final');rc=proc.wait(timeout=5);rt.join(timeout=1);errfile.close()
        dump(run/'final.json',final);result['host_exit_code']=rc
        assert len([r for r in server_rows if r['event']=='post_received'])==1
        assert any(r['event']=='actual_core_delta_observed_before_eof' for r in server_rows)
        assert any(r['event']=='http_eof_sent' for r in server_rows)
        progress=final['snapshot']['http']['progress'];result['progress']=progress
        assert progress['intent']=='Observed' and progress['http_eof'] and progress['response_material_stored']
        assert progress['worker_joined'] and progress['request_closed'] and progress['owner_released']
        assert not final['state']['owner']['owner_retained'] and final['snapshot']['exit_code']==0
        assert rc==0
        assert not any(row['event']=='server_error' for row in server_rows)
        assert request_count==1
        order={row['event']:row['at_ns'] for row in server_rows}
        assert order['post_received']<=order['first_sse_written']<=order['actual_core_delta_observed_before_eof']<order['http_eof_sent']
        result['delta_before_eof_margin_ns']=order['http_eof_sent']-order['actual_core_delta_observed_before_eof']
        expected_parser=sse({'type':'response.created','response':{'id':'resp_m03'}})+sse({'type':'response.output_text.delta','delta':'M03 synthetic delta'})+sse({'type':'response.completed','response':{'id':'resp_m03','usage':{'input_tokens':1,'output_tokens':1,'total_tokens':2}}})
        tail=b': drain tail\n\n';expected_response=expected_parser+tail
        (run/'fixture-response.bin').write_bytes(expected_response)
        n=len(expected_response);assert n==281
        for key in ['received_offset','reserved_offset','issued_offset','os_completed_offset','peer_consumed_offset']:assert progress[key]==n,(key,progress[key])
        assert progress['parser_yielded_bytes']==len(expected_parser)==267 and progress['drain_discarded_bytes']==len(tail)==14
        assert progress['error_consumed_bytes']==progress['cancel_discarded_bytes']==progress['error_code']==0
        for key in ['child_exited','stdout_eof','stderr_eof','connect_reaped','read_reaped','write_reaped','data_closed','worker_started']:assert progress[key]
        assert final['snapshot']['event_overflow']==0
        guest_result=json.loads((evidence/'result.json').read_text());audit=guest_result['task']['audit'];cleanup=guest_result['task']['cleanup']['Ok']
        assert guest_result['identity']['child_pid']==child_identity['pid'] and guest_result['identity']['session']==child_identity['session']
        assert guest_result['identity']['epoch']==child_identity['epoch'] and guest_result['identity']['artifact_sha256']==sha(guest)
        assert guest_result['close_result']==guest_result['final_control_result']=={'Ok':None}
        assert guest_result['result_snapshot']=='after_close_observation' and guest_result['local_data_thread_joined']
        assert not guest_result['frames_overflow'] and not guest_result['io_overflow'] and not audit['overflow']
        assert guest_result['task']['core_terminal']=={'Completed':{'end_turn':None}} and guest_result['task']['transport_drain']=={'Ok':None}
        assert audit['received_bytes']==audit['ack_queued_offset']==n
        assert audit['parser_yielded_bytes']==267 and audit['drain_discarded_bytes']==14 and audit['error_body_consumed_bytes']==0
        assert audit['first_cancel_reason'] is None and audit['cancel_queue_error'] is None
        for key in ['received_offset','delivered_offset','acknowledged_offset']:assert cleanup[key]==n
        for key in ['request_closed','network_eof','network_worker_started','network_worker_exited','data_channel_closed','data_connect_reaped','data_read_reaped','data_write_reaped','durable_observed']:assert cleanup[key]
        assert cleanup['session_release'] is None
        prepared=[x for x in audit['observations'] if x['kind']=='core_prepared_request'];assert len(prepared)==1 and prepared[0]['bytes']==len(raw) and prepared[0]['sha256']==hashlib.sha256(raw).hexdigest()
        core_events=[json.loads(line) for line in (evidence/'core-events.jsonl').read_text().splitlines()]
        deltas=[x for x in core_events if x['event']['kind']=='actual_core_output_text_delta'];assert len(deltas)==1 and deltas[0]['event']['sha256']==hashlib.sha256(b'M03 synthetic delta').hexdigest()
        completed=[x for x in core_events if x['event']['kind']=='actual_core_completed'];assert len(completed)==1
        names=[x['event'] for x in final['snapshot']['events']];assert names.index('close_ack_written')<names.index('exit') and 'close_ack_failed' not in names
        result['held_guest_final']=observe_guest(guest_handle[0]);assert result['held_guest_final']['wait_signaled'] and result['held_guest_final']['exit_code']==0
        result['guest_files_sha256']={name:sha(evidence/name) for name in ['result.json','core-events.jsonl']}
        result['byte_accounting']={'total':n,'parser':267,'drain':14,'cancel_discard':0,'error_consume':0,'all_offsets_equal':True}
        result['guest_final_control_ok']=True;result['actual_core_events']=len(core_events)
        result['status']='single_positive_core_http_pair_passed_not_full_m03'
    except Exception as e:result['error']=repr(e)
    finally:
        if proc is not None and proc.poll() is None:
            try:proc.stdin.write('{"action":"stop"}\n');proc.stdin.flush();proc.wait(timeout=15)
            except Exception as e:result['cleanup_error']=repr(e)
        if proc is not None and proc.stdin is not None:
            try:proc.stdin.close()
            except OSError as e:result['stdin_close_error']=repr(e)
        if errfile is not None:
            errfile.close()
        if rt is not None and proc.poll() is not None:
            rt.join(timeout=1)
        server.shutdown();server.server_close();server_thread.join(timeout=5)
        result['fixture_server_joined']=not server_thread.is_alive()
        if guest_handle is not None:
            result['held_guest_final']=observe_guest(guest_handle[0]);KERNEL.CloseHandle(guest_handle[0])
        dump(run/'host-rows.json',rows);dump(run/'operator-commands.json',commands);dump(run/'server-events.json',server_rows)
        if final is None:final=next((r['value'] for r in reversed(rows) if r['value'].get('event')=='final'),None)
        if final is not None:
            dump(run/'retained-final.json',final)
            result['progress']=final['snapshot']['http']['progress']
            result['root_close_reasons']=[e for e in final['snapshot']['events'] if e['event'] in ['session_close_reason','request_denied','pipe_driver_error','network_supervision_error']]
            result['timing']['host_events_original_approval_clock']=[e for e in final['snapshot']['events'] if e['event'] in ['spawn','proposal_complete','http_approved','send_fence','session_close_reason','close_ack_written','exit']]
        result['host_os_exit']=None if proc is None else proc.poll()
        result['host_lifetime']={'pid':None if proc is None else proc.pid,'status':'not_started' if proc is None else ('running_cleanup_unconfirmed' if proc.poll() is None else 'os_exit_observed'),'child_identity':child_identity,'owner_release_verified':bool(final and final.get('state',{}).get('owner',{}).get('owner_retained') is False)}
        result['inputs_unchanged']=all(sha(ROOT/p)==h for p,h in inputs.items()) and all(sha(PLUGIN/p)==h for p,h in pm['input_sha256'].items()) and sha(host)==frozen['host_sha256']
        if not result['inputs_unchanged'] or not result['fixture_server_joined'] or result.get('cleanup_error') or result.get('host_os_exit') is None:
            result['status']='failed_preserved_no_retry'
        dump(run/'result.json',result);print(json.dumps(result))
    return 0 if result['status'].startswith('single_positive') else 1
if __name__=='__main__':sys.exit(main())
