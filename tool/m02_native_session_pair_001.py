"""Real frozen plugin/host pairing. No fixture client substituted."""
import pathlib,json,hashlib,os,subprocess,threading,queue,time,datetime
ROOT=pathlib.Path(__file__).resolve().parents[1]
PLUGIN=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    handoff=PLUGIN/'receipts/m02-native-session-001/candidate-001.json'
    assert sha(handoff)=='9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'
    candidate=json.loads(handoff.read_text(encoding='utf-8'))
    def inputs():
        actual={name:sha(PLUGIN/name) for name in candidate['input_sha256']};assert actual==candidate['input_sha256'];return actual
    before=inputs();host=ROOT/'target/m02-native-session-001/debug/morrow-native-session-host.exe';client=pathlib.Path(candidate['exe'])
    run=ROOT/'reports/codex-morrow-v1.1/host/m02-native-session-001'/('pair-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir()
    report={'status':'failed','client_manifest_sha256':sha(handoff),'client_inputs_before':before,'host_sha256':sha(host),'client_sha256':sha(client),'cases':[]}
    try:
        for name in ['normal','revoke','stop']:
            cwd=run/name;cwd.mkdir();profile=run/(name+'-profile');profile.mkdir()
            env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC'] if k in os.environ}
            env.update({'HOME':str(profile),'USERPROFILE':str(profile),'APPDATA':str(profile),'LOCALAPPDATA':str(profile),'TEMP':str(profile),'TMP':str(profile)})
            count='2' if name=='normal' else '16'
            argv=[str(host),'--client',str(client),'--sha256',sha(client),'--work-dir',str(cwd),'--ttl-ms','8000','--client-arg','--queries','--client-arg',count,'--client-arg','--interval-ms','--client-arg','200']
            proc=subprocess.Popen(argv,cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            q=queue.Queue();err=[]
            def reader():
                for line in proc.stdout:q.put(line)
                q.put(None)
            def stderr_reader():err.append(proc.stderr.read())
            a=threading.Thread(target=reader,daemon=True);b=threading.Thread(target=stderr_reader,daemon=True);a.start();b.start()
            rows=[];raw=[];sent=False;started=time.monotonic()
            while True:
                line=q.get(timeout=12)
                if line is None:break
                raw.append(line);item=json.loads(line);rows.append(item)
                obs=item.get('observation',{})
                if name!='normal' and not sent and obs.get('event')=='state_read':
                    proc.stdin.write((json.dumps({'action':name})+'\n').encode());proc.stdin.flush();sent=True
            code=proc.wait(timeout=3);proc.stdin.close();a.join(1);b.join(1)
            (run/(name+'.stdout.jsonl')).write_bytes(b''.join(raw));(run/(name+'.stderr')).write_bytes(b''.join(err))
            final=rows[-1]['snapshot'];assert rows[-1]['event']=='final' and code==0
            assert final['phase']=='Released' and final['exit_observed'] and final['stdout_eof'] and final['stderr_eof'] and not final['owner_retained'] and final['event_overflow']==0
            events=final['events'];assert any(e['event']=='state_read' for e in events)
            if name=='normal':assert final['exit_code']==0
            if name=='revoke':
                assert final['exit_code']==19
                assert any(e['event']=='request_denied' and e['detail']['code']==19 for e in events)
                ack=next(i for i,e in enumerate(events) if e['event']=='control_ack')
                assert not any(e['event']=='state_read' for e in events[ack+1:])
            if name=='stop':assert final['exit_code']==0 and sent
            report['cases'].append({'name':name,'host_pid':proc.pid,'host_exit':code,'elapsed_seconds':time.monotonic()-started,'stdin_held_open_until_exit':True,'snapshot':final})
        report['client_inputs_after']=inputs();assert sha(host)==report['host_sha256'];report['status']='passed_real_frozen_plugin_host_pair'
    except Exception as e:report['error']=repr(e)
    (run/'result.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print(json.dumps({'status':report['status'],'path':str(run/'result.json'),'error':report.get('error')}))
    return 0 if report['status'].startswith('passed') else 1
if __name__=='__main__':raise SystemExit(main())
