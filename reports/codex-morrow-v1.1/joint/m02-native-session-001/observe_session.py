"""Independent bounded process/pipe observer, called only after ready source audit.

Launch arguments are supplied by the review driver, never by peer frames.
Raw host traces remain host observations; OS process facts are independently sampled.
"""
import json,os,queue,subprocess,threading,time
from pathlib import Path
from check_review import HERE,sha,write
from process_witness import ProcessWitness
from runtime_decode import decode_frame
from watch_owned_processes import OwnedWatcher

def observe(run,host,host_sha,client,client_sha,client_args=(),ttl_ms=4000,
            handshake_ms=1000,frame_ms=500,close_ms=200,budget=64,control=None,limit_s=10):
    run=Path(run).resolve()
    if not run.is_relative_to(HERE/'runs'):raise ValueError('outside joint run scope')
    run.mkdir(exist_ok=False)
    if sha(host)!=host_sha or sha(client)!=client_sha:raise ValueError('launch artifact changed')
    for name in ['work','profile','tmp']:(run/name).mkdir()
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'] if k in os.environ}
    env.update(PATH=str(Path(env.get('SYSTEMROOT','C:/Windows'))/'System32'),
      HOME=str(run/'profile'),USERPROFILE=str(run/'profile'),APPDATA=str(run/'profile'),LOCALAPPDATA=str(run/'profile'),
      TEMP=str(run/'tmp'),TMP=str(run/'tmp'),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='NUL',GIT_CONFIG_SYSTEM='NUL')
    args=[str(host),'--client',str(client),'--sha256',client_sha,'--work-dir',str(run/'work'),
          '--ttl-ms',str(ttl_ms),'--handshake-ms',str(handshake_ms),'--frame-ms',str(frame_ms),
          '--close-ms',str(close_ms),'--budget',str(budget)]
    for arg in client_args:args.extend(['--client-arg',str(arg)])
    started=time.monotonic_ns()
    proc=subprocess.Popen(args,cwd=run,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                          creationflags=subprocess.CREATE_NO_WINDOW,bufsize=0)
    records=[]; events=queue.Queue(maxsize=8192); drains={};workers=[]
    def drain(stream,name):
        state={'eof':False,'bytes':0,'dropped_bytes':0,'queue_drops':0};drains[name]=state
        with (run/f'host.{name}').open('wb') as log:
            while True:
                raw=stream.readline(2*1024*1024+1)
                if not raw:state.update(eof=True,eof_at_ns=time.monotonic_ns());break
                state['bytes']+=len(raw)
                keep=max(0,min(len(raw),16*1024*1024-log.tell()))
                log.write(raw[:keep]);state['dropped_bytes']+=len(raw)-keep
                if name=='stdout':
                    item={'observed_at_ns':time.monotonic_ns(),'raw':raw}
                    try:events.put_nowait(item)
                    except queue.Full:state['queue_drops']+=1
            log.flush()
    for name in ['stdout','stderr']:
        t=threading.Thread(target=drain,args=(getattr(proc,name),name),daemon=True);t.start();workers.append(t)
    host_witness=None;child_witness=None;child_pid=None;final=None;final_at=None;sent=set();failure=[];frames=[]
    host_os=[];child_os=[];controller=[];open_stdin_exit=False;watcher=None;descendants=None
    def send(action):
        if action not in {'inspect','revoke','stop'}:raise ValueError('invalid reviewer control')
        data=(json.dumps({'action':action})+'\n').encode()
        proc.stdin.write(data);proc.stdin.flush()
        controller.append({'action':action,'sent_at_ns':time.monotonic_ns(),'raw_hex':data.hex()});sent.add(action)
    try:
        host_witness=ProcessWitness(proc.pid,os.getpid());host_os.append(host_witness.snapshot())
        watcher=OwnedWatcher(proc.pid,client,host_os[0]['creation_filetime_100ns'])
        deadline=time.monotonic()+limit_s
        while True:
            try:item=events.get(timeout=.02)
            except queue.Empty:item=None
            if item:
                try:event=json.loads(item['raw'])
                except (json.JSONDecodeError,UnicodeDecodeError):
                    failure.append('non_json_host_evidence');continue
                record={'observed_at_ns':item['observed_at_ns'],'event':event};records.append(record)
                if event.get('event')=='host_observation':
                    pid=event['pid']
                    if child_pid is None:
                        child_pid=pid
                        watcher.add_reported_child(pid)
                        try:
                            child_witness=ProcessWitness(pid,proc.pid)
                            sample=child_witness.snapshot()
                            if Path(sample['image_path']).resolve()!=Path(client).resolve():raise ValueError('wrong child image path')
                            if sample['creation_filetime_100ns']<host_os[0]['creation_filetime_100ns']:raise ValueError('child predates host')
                            sample['image_path_sha256']=sha(sample['image_path']);child_os.append(sample)
                        except (OSError,ValueError) as exc:
                            failure.append('child_OS_observation_unavailable:'+type(exc).__name__)
                    elif pid!=child_pid:raise ValueError('unexpected child PID change')
                    obs=event['observation'];kind=obs['event'];detail=obs.get('detail')
                    if kind in {'frame_sent','frame_received'}:
                        raw=bytes.fromhex(detail['raw_hex'])
                        try:decoded,_,_=decode_frame(raw)
                        except ValueError as exc:decoded={'decode_error':str(exc)}
                        frames.append({'direction':kind,'host_at_us':obs['at_us'],'observed_at_ns':item['observed_at_ns'],
                                       'raw_hex':raw.hex(),'decoded':decoded})
                        if control in {'revoke','stop'} and kind=='frame_sent' and decoded.get('kind')=='state' and control not in sent:
                            send(control)
                    if kind=='request_denied' and control=='revoke' and 'stop' not in sent:
                        # Let Denied reach the client; normal client exits by itself.
                        pass
                if event.get('event')=='final':final=event['snapshot'];final_at=time.monotonic_ns()
            if proc.poll() is not None and events.empty() and all(s.get('eof') for s in drains.values()) and len(drains)==2:break
            if final_at is not None and proc.poll() is None and (time.monotonic_ns()-final_at)>1_500_000_000:
                failure.append('host_failed_to_exit_with_controller_stdin_open');break
            if time.monotonic()>=deadline:failure.append('reviewer_deadline_exceeded');break
        open_stdin_exit=proc.poll() is not None and not proc.stdin.closed
        if proc.poll() is None:
            try:send('stop')
            except OSError:pass
            try:proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                # Cleanup is not counted as the host's bounded-release success.
                proc.stdin.close()
                try:proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    proc.kill();proc.wait(timeout=2);failure.append('reviewer_forced_host_termination')
        if not proc.stdin.closed:proc.stdin.close()
        for t in workers:t.join(timeout=1)
        if host_witness:host_os.append(host_witness.snapshot())
        if child_witness:child_os.append(child_witness.snapshot())
        if any(t.is_alive() for t in workers):failure.append('observer_output_EOF_unconfirmed')
        if child_witness and not child_os[-1]['process_handle_signaled']:failure.append('child_exit_independently_unconfirmed')
    except Exception as exc:
        failure.append('observer_error:'+type(exc).__name__+':'+str(exc))
        if proc.poll() is None:
            try:send('stop')
            except OSError:pass
            if not proc.stdin.closed:proc.stdin.close()
            try:proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill();proc.wait(timeout=2);failure.append('reviewer_forced_host_termination')
        for t in workers:t.join(timeout=1)
        if host_witness:host_os.append(host_witness.snapshot())
        if child_witness:child_os.append(child_witness.snapshot())
    finally:
        if watcher:descendants=watcher.finish()
        if host_witness:host_witness.close()
        if child_witness:child_witness.close()
    result={'scope':'independent_real_host_and_child_observation_not_product_acceptance',
      'argv':args,'host_pid':proc.pid,'child_pid':child_pid,'host_sha256':host_sha,'client_sha256':client_sha,
      'environment_keys':sorted(env),'environment_values_enumerated':False,
      'started_monotonic_ns':started,'finished_monotonic_ns':time.monotonic_ns(),
      'host_exit_code':proc.returncode,'controller_stdin_open_at_host_exit':open_stdin_exit,
      'host_OS_observations':host_os,'child_OS_observations':child_os,
      'descendant_OS_observations':descendants,
      'host_output_observations':drains,'controller_commands':controller,
      'final_host_snapshot':final,'failures':failure,'frames':frames,
      'frame_provenance':'host captured actual pipe bytes; independently decoded and cross-checked; not external pipe interception',
      'child_EOF_provenance':'host reader evidence only; host stdout/stderr EOF independently observed',
      'status':'observed_pending_semantic_review','product_pass_credit':0}
    write(run/'host-events.json',records);write(run/'result.json',result)
    return result
