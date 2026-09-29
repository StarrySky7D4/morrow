"""One new strict Welcome-code real child/control-only check. No plugin/Core/HTTP request."""
import datetime,hashlib,json,os,pathlib,queue,shutil,subprocess,threading,time
ROOT=pathlib.Path(__file__).resolve().parents[1];BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def dump(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def main():
    run=BASE/('native-welcome-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir();candidate=run/'candidate';candidate.mkdir()
    host=candidate/'morrow-native-stream-host.exe';peer=candidate/'morrow-native-close-peer.exe'
    for p in [host,peer]:shutil.copy2(ROOT/'target/m03-stream-001-native/debug'/p.name,p)
    inputs={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted((ROOT/'native_session_stream_001').rglob('*')) if p.is_file()}
    dump(candidate/'manifest.json',{'source_sha256':inputs,'host_sha256':sha(host),'peer_sha256':sha(peer),'script_sha256':sha(pathlib.Path(__file__)),'scope':'synthetic protocol peer only; no Core/HTTP/plugin'})
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','LOCALAPPDATA'] if k in os.environ};results=[]
    for mode in ['normal']:
        case=run/mode;case.mkdir();profile=case/'profile';profile.mkdir();cwd=case/'cwd';cwd.mkdir();evidence=case/'peer.json';rows=[];proc=None;r={'mode':mode,'passed':False}
        try:
            init=subprocess.run([str(host),'init','--profile',str(profile),'--slot','close-test'],env=env,capture_output=True,timeout=10);dump(case/'init.json',{'exit':init.returncode,'out':init.stdout.decode(),'err':init.stderr.decode()});assert init.returncode==0
            args=[str(host),'serve','--profile',str(profile),'--client',str(peer),'--sha256',sha(peer),'--work-dir',str(cwd),'--plugin-id','close-fixture','--role','test','--operation','close-test','--http-origin','http://127.0.0.1:1','--handshake-ms','6000','--ttl-ms','10000','--close-ms','1000','--client-arg',mode,'--client-arg',str(evidence)]
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
            p=json.loads(evidence.read_text());events=final['snapshot']['events'];names=[e['event'] for e in events];s=final['snapshot'];progress=s['http']['progress']
            assert s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'] and not final['state']['owner']['owner_retained']
            assert progress['data_closed'] and progress['connect_reaped'] and not progress['worker_started']
            if mode=='normal':
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
    result={'status':'passed' if all(r['passed'] for r in results) else 'failed','cases':results,'inputs_unchanged':all(sha(ROOT/p)==h for p,h in inputs.items()),'scope':'one strict Welcome code0/sequence1 and Close positive process case only; no HTTP/Core/plugin execution; old negative/API/component suites not rerun'}
    dump(run/'result.json',result);print(json.dumps({'path':str(run/'result.json'),**result}));return 0 if result['status']=='passed' else 1
if __name__=='__main__':raise SystemExit(main())
