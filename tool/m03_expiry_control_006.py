"""Three new real child/control-only Close checks. No plugin/Core/HTTP request."""
import datetime,hashlib,json,os,pathlib,queue,shutil,subprocess,threading,time
ROOT=pathlib.Path(__file__).resolve().parents[1];BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def dump(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def main():
    run=BASE/('expiry-control-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir();candidate=run/'candidate';candidate.mkdir()
    host=candidate/'morrow-native-stream-host.exe';peer=candidate/'morrow-native-close-peer.exe'
    for p in [host,peer]:shutil.copy2(ROOT/'target/m03-expiry-006-host/debug'/p.name,p)
    inputs={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted((ROOT/'native_session_stream_001').rglob('*')) if p.is_file()}
    dump(candidate/'manifest.json',{'source_sha256':inputs,'host_sha256':sha(host),'peer_sha256':sha(peer),'script_sha256':sha(pathlib.Path(__file__)),'scope':'synthetic protocol peer only; no Core/HTTP/plugin'})
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','LOCALAPPDATA'] if k in os.environ};results=[]
    for mode in ['normal','bad-sequence','post-close-bytes','expiry-normal','expiry-bad-sequence','expiry-bad-identity','expiry-bad-budget','expiry-positive-credit','expiry-no-close','expiry-partial','expiry-malformed','expiry-replay','expiry-tail','expiry-stderr-limit']:
        case=run/mode;case.mkdir();profile=case/'profile';profile.mkdir();cwd=case/'cwd';cwd.mkdir();evidence=case/'peer.json';rows=[];proc=None;r={'mode':mode,'passed':False}
        try:
            init=subprocess.run([str(host),'init','--profile',str(profile),'--slot','close-test'],env=env,capture_output=True,timeout=10);dump(case/'init.json',{'exit':init.returncode,'out':init.stdout.decode(),'err':init.stderr.decode()});assert init.returncode==0
            args=[str(host),'serve','--profile',str(profile),'--client',str(peer),'--sha256',sha(peer),'--work-dir',str(cwd),'--plugin-id','close-fixture','--role','test','--operation','close-test','--http-origin','http://127.0.0.1:1','--close-ms','1000','--ttl-ms','1200','--handshake-ms','500','--client-arg',mode,'--client-arg',str(evidence)]
            dump(case/'launch.json',{'args':args,'environment':env});q=queue.Queue();err=(case/'stderr.txt').open('w')
            proc=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,encoding='utf-8',env=env)
            def reader():
                for line in proc.stdout:
                    row=json.loads(line);rows.append(row);q.put(row)
                q.put(None)
            thread=threading.Thread(target=reader,daemon=True);thread.start()
            def receive(predicate):
                until=time.monotonic()+12
                while time.monotonic()<until:
                    v=q.get(timeout=max(.01,until-time.monotonic()))
                    if v is None:raise RuntimeError('host exited early')
                    if predicate(v):return v
                raise TimeoutError('missing result')
            def command(v):
                proc.stdin.write(json.dumps(v)+'\n');proc.stdin.flush();v=receive(lambda r:r.get('event')=='operator_result');assert v['ok'],v;return v['result']
            receive(lambda r:r.get('event')=='proposal');grant=command({'action':'approve'})['grant_id'];command({'action':'claim','grant_id':grant})
            final=receive(lambda r:r.get('event')=='final');rc=proc.wait(timeout=5);thread.join(timeout=1);err.close();dump(case/'final.json',final)
            try:p=json.loads(evidence.read_text())
            except (FileNotFoundError,json.JSONDecodeError):
                assert mode=='expiry-no-close', 'unexpected missing/corrupt peer evidence'
                p={'ack_received':False,'stop_received':False,'peer_evidence_unconfirmed':True}
            events=final['snapshot']['events'];names=[e['event'] for e in events];s=final['snapshot'];progress=s['http']['progress']
            assert s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'] and not final['state']['owner']['owner_retained']
            assert progress['data_closed'] and progress['connect_reaped'] and not progress['worker_started']
            if mode=='expiry-normal':
                assert rc==0 and s['exit_code']==0 and p['ack_received'] and p['stop_received']
                assert final['expired_terminal_protocol_complete'] is True
                assert progress['request_closed'] and progress['revoke_persisted'] and progress['revoke_applied']
                assert progress['error_code']==20 and 'close_ack_failed' not in names
                assert names.index('expiry_teardown_bound')<names.index('expiry_teardown_close_received')<names.index('close_ack_written')<names.index('exit')
                ack=next(e['detail'] for e in events if e['event']=='expiry_teardown_ack_written')
                assert ack['completed_offset_ns']<ack['deadline_offset_ns']
            elif mode.startswith('expiry-'):
                assert rc==2 and final['expired_terminal_protocol_complete'] is False
                assert 'expiry_teardown_bound' in names
                assert next(e['detail']['reason'] for e in events if e['event']=='session_close_reason')==20
                expected={
                    'expiry-bad-sequence':'teardown identity/sequence', 'expiry-bad-identity':'teardown identity/sequence',
                    'expiry-bad-budget':'teardown identity/sequence', 'expiry-positive-credit':'non-cleanup or late teardown request',
                    'expiry-no-close':'fixed teardown deadline', 'expiry-partial':'teardown partial frame timeout',
                    'expiry-malformed':'teardown framing', 'expiry-replay':'unexpected guest bytes after teardown Close',
                    'expiry-tail':'unexpected guest bytes after teardown Close', 'expiry-stderr-limit':'teardown stderr limit',
                }[mode]
                assert any(e['event']=='close_ack_failed' and e['detail']['reason']==expected for e in events), (mode,expected)
                if mode=='expiry-no-close':assert 'kill_requested' in names and 'close_ack_written' not in names
            elif mode=='normal':
                assert rc==0 and s['exit_code']==0 and p['ack_received'] and not p['stop_received']
                assert names.index('close_ack_pending')<names.index('close_ack_written')<names.index('exit')
                assert 'close_ack_failed' not in names
            elif mode=='bad-sequence':assert rc==2 and not p['ack_received'] and p['stop_received'] and 'close_ack_written' not in names
            else:assert rc==2 and 'close_ack_failed' in names
            r.update(passed=True,host_exit=rc,peer_exit=s['exit_code'],ack_received=p['ack_received'],owner_released=True,no_http_worker=True)
        except Exception as e:r['error']=repr(e)
        finally:
            if proc is not None and proc.poll() is None:
                try:proc.stdin.write('{"action":"stop"}\n');proc.stdin.flush();proc.wait(timeout=15)
                except Exception as e:r['cleanup_error']=repr(e)
            dump(case/'host-rows.json',rows);dump(case/'result.json',r);results.append(r)
    result={'status':'passed' if all(r['passed'] for r in results) else 'failed','cases':results,'inputs_unchanged':all(sha(ROOT/p)==h for p,h in inputs.items()),'scope':'real child/control-only original and expiry cleanup negative regressions; no OS partial-write injection; no HTTP/Core/plugin execution'}
    dump(run/'result.json',result);print(json.dumps({'path':str(run/'result.json'),**result}));return 0 if result['status']=='passed' else 1
if __name__=='__main__':raise SystemExit(main())
