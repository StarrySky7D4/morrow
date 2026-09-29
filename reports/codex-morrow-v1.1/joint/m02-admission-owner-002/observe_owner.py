"""Bounded observer for ready, explicitly pinned hosts. Does not enumerate PIDs.

No entrypoint: the independent driver must validate ready pins before constructing
an observer. Pipes/child creation remain host evidence; known PID OS facts separate.
"""
import ctypes,json,os,queue,subprocess,threading,time
from pathlib import Path
from ctypes import wintypes as W
from check_review import HERE,sha,write
from known_process import KnownProcess,filetime

def environment(root):
    root=Path(root).resolve()
    if not root.is_relative_to(HERE/'runs'):raise ValueError('environment outside joint run')
    for name in ['os-profile','tmp']:(root/name).mkdir(exist_ok=True)
    env={k:os.environ[k]for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS']if k in os.environ}
    env.update(PATH=str(Path(env.get('SYSTEMROOT','C:/Windows'))/'System32'),HOME=str(root/'os-profile'),USERPROFILE=str(root/'os-profile'),APPDATA=str(root/'os-profile'),LOCALAPPDATA=str(root/'os-profile'),TEMP=str(root/'tmp'),TMP=str(root/'tmp'),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='NUL',GIT_CONFIG_SYSTEM='NUL')
    return env

def short_command(folder,host,digest,args,env):
    folder=Path(folder).resolve()
    if not folder.is_relative_to(HERE/'runs') or sha(host)!=digest:raise ValueError('command scope/identity')
    folder.mkdir(exist_ok=False)
    if args[0] not in ['init','inspect']:raise ValueError('only trusted initialization/inspection')
    p=subprocess.run([str(host)]+list(map(str,args)),cwd=folder,env=env,input=b'',capture_output=True,timeout=6,creationflags=subprocess.CREATE_NO_WINDOW)
    (folder/'stdout').write_bytes(p.stdout);(folder/'stderr').write_bytes(p.stderr)
    result={'argv':[str(host)]+list(map(str,args)),'exit_code':p.returncode,'stdout_sha256':sha(folder/'stdout'),'stderr_sha256':sha(folder/'stderr'),'rows':[json.loads(x)for x in p.stdout.splitlines()],'scope':'short initialization/inspection; not child runtime proof'}
    write(folder/'result.json',result);return result

class Host:
    def __init__(self,folder,host,host_sha,client,client_sha,args,env):
        self.folder=Path(folder).resolve();self.host=Path(host);self.client=Path(client);self.host_sha=host_sha;self.client_sha=client_sha
        if not self.folder.is_relative_to(HERE/'runs') or sha(host)!=host_sha or sha(client)!=client_sha:raise ValueError('host/client scope or hash changed')
        self.folder.mkdir(exist_ok=False);self.argv=[str(host),'serve']+list(map(str,args));self.started_ns=time.monotonic_ns()
        self.proc=subprocess.Popen(self.argv,cwd=self.folder,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW,bufsize=0)
        self.records=[];self.commands=[];self.failures=[];self.child=None;self.holder=None;self.os_rows=[];self.drains={};self.workers=[];self.queue=queue.Queue(maxsize=8192);self.finished=False;self.crashed=False
        self.host_os=KnownProcess(self.proc.pid,host,host_sha,'reviewer subprocess.Popen returned host PID')
        self.os_rows.append({'role':'host','stage':'initial','snapshot':self.host_os.snapshot()})
        def drain(stream,name):
            state={'eof':False,'bytes':0,'dropped_bytes':0,'queue_drops':0};self.drains[name]=state
            with (self.folder/('host.'+name)).open('wb') as f:
                while True:
                    line=stream.readline(256*1024+1)
                    if not line:state.update(eof=True,eof_at_ns=time.monotonic_ns());break
                    state['bytes']+=len(line);keep=max(0,min(len(line),16*1024*1024-f.tell()));f.write(line[:keep]);state['dropped_bytes']+=len(line)-keep
                    if name=='stdout':
                        try:self.queue.put_nowait((time.monotonic_ns(),line))
                        except queue.Full:state['queue_drops']+=1
        for name in ['stdout','stderr']:
            t=threading.Thread(target=drain,args=(getattr(self.proc,name),name),daemon=True);t.start();self.workers.append(t)
    def pump(self,timeout=.01):
        try:ns,line=self.queue.get(timeout=timeout)
        except queue.Empty:return
        try:event=json.loads(line)
        except Exception:self.failures.append('invalid host JSON evidence');return
        self.records.append({'observed_at_ns':ns,'event':event})
        pid=None
        if event.get('event')=='operator_result' and event.get('action')=='claim' and event.get('ok'):pid=event['result']['pid']
        elif event.get('event')=='host_observation':pid=event['pid']
        if pid:
            if self.child is None:
                try:
                    self.child=KnownProcess(pid,self.client,self.client_sha,'pinned host claim/spawn event; parentage not independently queried',self.host_os.created)
                    self.os_rows.append({'role':'child','stage':'initial','snapshot':self.child.snapshot()})
                except (OSError,ValueError) as e:self.failures.append('child_OS_unavailable:'+type(e).__name__)
            elif pid!=self.child.pid:raise ValueError('unexpected second child PID in one serve')
    def wait(self,predicate,timeout=5,start=0):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            for x in self.records[start:]:
                if predicate(x['event']):return x
            if self.proc.poll() is not None and self.queue.empty() and self.drains.get('stdout',{}).get('eof'):break
            self.pump(.01)
        raise TimeoutError('expected host evidence not observed')
    def send(self,action,grant=None,extra=None):
        if action not in ['approve','claim','revoke','inspect','stop','quit']:raise ValueError('unsupported operator action')
        row={'action':action}
        if grant is not None:row['grant_id']=grant
        if extra:row.update(extra)
        mark=len(self.records);raw=(json.dumps(row,separators=(',',':'))+'\n').encode()
        if len(raw)>1024:raise ValueError('command length')
        before=time.monotonic_ns();self.proc.stdin.write(raw);self.proc.stdin.flush()
        self.commands.append({'sent_at_ns':before,'action':action,'request':row,'record_cursor':mark})
        return mark
    def command(self,action,grant=None,extra=None,timeout=5):
        start=self.send(action,grant,extra)
        return self.wait(lambda e:e.get('event')=='operator_result' and e.get('action')==action,timeout,start)['event']
    def snapshot_os(self,stage):
        for role,witness in [('host',self.host_os),('child',self.child),('holder',self.holder)]:
            if witness:self.os_rows.append({'role':role,'stage':stage,'snapshot':witness.snapshot()})
    def attach_holder(self,path):
        path=Path(path).resolve()
        if not path.is_relative_to(HERE/'runs') or path.stat().st_size>2048:raise ValueError('holder PID file scope')
        d=json.loads(path.read_text())
        if not self.child or d['root_pid']!=self.child.pid or d['source']!='peer_owned_child_handle':raise ValueError('holder provenance')
        self.holder=KnownProcess(d['holder_pid'],self.client,self.client_sha,'peer holder.json '+str(path)+' SHA256 '+sha(path),self.child.created)
        self.os_rows.append({'role':'holder','stage':'initial','snapshot':self.holder.snapshot()})
    def crash_own_host(self):
        if not self.child or self.child.snapshot()['handle_signaled'] or self.host_os.snapshot()['handle_signaled']:raise ValueError('must witness live own host and child before crash')
        self.snapshot_os('before_crash');self.crashed=True
        self.commands.append({'action':'reviewer_terminate_own_host','sent_at_ns':time.monotonic_ns(),'host_pid':self.proc.pid,'scope':'test crash injection only'})
        self.proc.kill()
    def wait_exit(self,timeout=6):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            self.pump(.01)
            if self.proc.poll() is not None and self.queue.empty() and len(self.drains)==2 and all(v['eof']for v in self.drains.values()):
                for t in self.workers:t.join(timeout=.5)
                self.snapshot_os('host_exit');return self.proc.returncode
        raise TimeoutError('host did not exit/drain within reviewer bound')
    def save(self):
        if self.finished:return
        self.snapshot_os('final')
        result={'status':'observed_pending_semantic_review','argv':self.argv,'host_pid':self.proc.pid,'host_sha256':self.host_sha,'client_sha256':self.client_sha,'started_monotonic_ns':self.started_ns,'finished_monotonic_ns':time.monotonic_ns(),'host_exit_code':self.proc.poll(),'controller_stdin_open_at_host_exit':self.proc.poll() is not None and not self.proc.stdin.closed,'reviewer_crash_injected':self.crashed,'commands':self.commands,'OS_observations':self.os_rows,'drains':self.drains,'failures':self.failures,'known_child_count':int(self.child is not None),'known_holder_count':int(self.holder is not None),'provenance':'host captures child IPC/EOF; explicit known-PID OS handles independently observed; no PPID or whole-tree enumeration','product_pass_credit':0}
        write(self.folder/'events.json',self.records);write(self.folder/'result.json',result)
        if not self.proc.stdin.closed:self.proc.stdin.close()
        for w in [self.host_os,self.child,self.holder]:
            if w:w.close()
        self.finished=True
    def cleanup(self):
        if self.proc.poll() is None:
            try:self.command('quit',timeout=1)
            except (OSError,TimeoutError):pass
            try:self.wait_exit(2)
            except TimeoutError:
                self.failures.append('reviewer_forced_host_cleanup');self.proc.kill();self.wait_exit(2)
        else:
            try:self.wait_exit(2)
            except TimeoutError:self.failures.append('host_output_drain_unconfirmed_during_cleanup')
        # Source-bounded peers should exit by themselves. Any forced cleanup remains
        # a reviewer action and never counts as product recovery/release evidence.
        for role,w in [('child',self.child),('holder',self.holder)]:
            if w and not w.snapshot()['handle_signaled']:
                deadline=time.monotonic()+11
                while time.monotonic()<deadline and not w.snapshot()['handle_signaled']:time.sleep(.02)
                if not w.snapshot()['handle_signaled']:
                    terminate_exact(w);self.failures.append('reviewer_forced_'+role+'_cleanup')
        self.save()

def terminate_exact(witness):
    """Finite cleanup only of an already witnessed process created in this batch."""
    k=witness.k;h=k.OpenProcess(0x1000|0x00100000|1,False,witness.pid)
    if not h:raise ctypes.WinError(ctypes.get_last_error())
    try:
        c,e,kt,ut=(W.FILETIME()for _ in range(4))
        if not k.GetProcessTimes(h,*[ctypes.byref(v)for v in [c,e,kt,ut]]) or filetime(c)!=witness.created or k.GetProcessId(h)!=witness.pid:raise ValueError('refuse cleanup: changed process identity')
        fn=k.TerminateProcess;fn.argtypes=[W.HANDLE,W.UINT];fn.restype=W.BOOL
        if not fn(h,98):raise ctypes.WinError(ctypes.get_last_error())
        if k.WaitForSingleObject(h,2000)!=0:raise TimeoutError('owned process cleanup unconfirmed')
    finally:k.CloseHandle(h)
