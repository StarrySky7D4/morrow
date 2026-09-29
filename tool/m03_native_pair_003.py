"""One fresh synthetic loopback POST; independently hash original proposal bytes.
All candidates and evidence are append-only. Never retries a request/profile.
"""
import datetime, hashlib, http.server, json, os, pathlib, queue, shutil, struct, subprocess, sys, threading, time
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
PLUGIN=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def dump(p,v): p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def sse(v): return ('data: '+json.dumps(v,separators=(',',':'))+'\n\n').encode()
def main():
    if sys.argv[1:] != ['--coordinator-resumed']:raise RuntimeError('pairing requires coordinator resume for fixed host002/plugin002 diagnostic003')
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    run=BASE/('native-pair-'+stamp);run.mkdir()
    evidence=PLUGIN/'out/m03-host-003'/stamp;evidence.mkdir(parents=True)
    profile=run/'profile';profile.mkdir();cwd=run/'guest-cwd';cwd.mkdir()
    candidate=run/'candidate';candidate.mkdir();host=candidate/'morrow-native-stream-host.exe'
    shutil.copy2(BASE/'native-host-candidate-002/morrow-native-stream-host.exe',host)
    host_manifest=BASE/'native-host-candidate-002/manifest.json'
    assert sha(host_manifest)=='12b20faeee5e3bd352d8ee204ba7d2d155a9b416e5f8fa3edbf922bf5367a021'
    hm=json.loads(host_manifest.read_text());assert sha(host)==hm['executable_sha256']
    for p,h in hm['source_files'].items():assert sha(host_manifest.parent/'source'/p)==h,p
    inputs={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted((ROOT/'native_session_stream_001').rglob('*')) if p.is_file()}
    for source in ['network_node_stream_001','contracts/experimental/agent_host_v3_http_stream','native_pipe_win_001']:
        inputs.update({str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted((ROOT/source).rglob('*')) if p.is_file() and 'target' not in p.parts})
    manifest=PLUGIN/'receipts/m03-stream-002/native-candidate-002.json'
    assert sha(manifest)=='20135ba780b86a344eeb396fd8d8f0ba85ee3600f79308140380c3fc94fbf561'
    pm=json.loads(manifest.read_text());guest=pathlib.Path(pm['executable'])
    assert sha(guest)==pm['executable_sha256']=='f59665f639f3f6f9157111e4811cccc32569ec47cc63850fe936c13dddd5d32e'
    for p,h in pm['input_sha256'].items(): assert sha(PLUGIN/p)==h,p
    frozen={'host_sha256':sha(host),'host_manifest_sha256':sha(host_manifest),'source_sha256':inputs,'plugin_manifest_sha256':sha(manifest),'plugin_executable_sha256':sha(guest),'harness_sha256':sha(pathlib.Path(__file__)),'harness_revision':'diagnostic003: explicit handshake 6000ms within original 10000ms; final/exit-aware commands and root cause preservation; fresh grant/profile/op; no retry', 'handshake_ms':6000, 'original_ttl_ms':10000,'no_post_retry':True}
    dump(candidate/'manifest.json',frozen)
    start=time.monotonic_ns();server_rows=[];approved_body=None
    def mark(event,**kw):server_rows.append({'at_ns':time.monotonic_ns()-start,'event':event,**kw})
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version='HTTP/1.1'
        def log_message(self,*a):pass
        def do_POST(self):
            try:
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
                    if f.exists() and 'actual_core_output_text_delta' in f.read_text():break
                    time.sleep(.005)
                else:raise RuntimeError('actual Core delta not observed before EOF gate deadline')
                mark('actual_core_delta_observed_before_eof')
                chunk(sse({'type':'response.completed','response':{'id':'resp_m03','usage':{'input_tokens':1,'output_tokens':1,'total_tokens':2}}}))
                # Core completion is followed by a bounded transport-only tail.
                chunk(b': drain tail\n\n');time.sleep(.03)
                self.wfile.write(b'0\r\n\r\n');self.wfile.flush();mark('http_eof_sent')
            except Exception as e:mark('server_error',error=repr(e));self.close_connection=True
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=True
    port=server.server_address[1];threading.Thread(target=server.serve_forever,daemon=True).start()
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','LOCALAPPDATA'] if k in os.environ}
    result={'status':'failed','run':str(run),'plugin_evidence':str(evidence),'candidate_manifest_sha256':sha(candidate/'manifest.json'),'handshake_ms':6000,'original_ttl_ms':10000,'timing':{}}
    proc=None;errfile=None;rt=None;final=None;rows=[];commands=[];child_identity=None
    try:
        init=subprocess.run([str(host),'init','--profile',str(profile),'--slot','m03-slot'],env=env,cwd=ROOT,capture_output=True,timeout=10)
        (run/'init.stdout').write_bytes(init.stdout);(run/'init.stderr').write_bytes(init.stderr);assert init.returncode==0,init.stderr
        operation='m03-'+stamp
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
        server.shutdown();server.server_close()
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
        dump(run/'result.json',result);print(json.dumps(result))
    return 0 if result['status'].startswith('single_positive') else 1
if __name__=='__main__':sys.exit(main())
