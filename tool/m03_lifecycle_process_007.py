"""Owned synthetic Windows processes only. No HTTP, automatic takeover, or user PID discovery."""
from pathlib import Path
import argparse,ctypes,datetime,hashlib,json,os,queue,shutil,subprocess,threading,time
ROOT=Path(__file__).resolve().parents[1]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def dump(p,v):p.write_bytes((json.dumps(v,indent=2)+'\n').encode())
def load(p):return json.loads(p.read_text(encoding='utf-8'))
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.c_uint32];k.OpenProcess.restype=ctypes.c_void_p
k.GetProcessId.argtypes=[ctypes.c_void_p];k.GetProcessId.restype=ctypes.c_uint32
k.GetExitCodeProcess.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint32)]
k.TerminateProcess.argtypes=[ctypes.c_void_p,ctypes.c_uint32]
k.WaitForSingleObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32]
k.CloseHandle.argtypes=[ctypes.c_void_p]
class OwnedProcess:
 def __init__(self,pid):
  self.pid=pid;self.h=k.OpenProcess(0x100000|0x1000|1,False,pid)
  assert self.h and k.GetProcessId(self.h)==pid,'cannot retain exact test PID handle'
 def alive(self):
  v=ctypes.c_uint32();assert k.GetExitCodeProcess(self.h,ctypes.byref(v));return v.value==259
 def finish(self):
  if self.alive():assert k.TerminateProcess(self.h,71),'test cleanup terminate failed'
  assert k.WaitForSingleObject(self.h,5000)==0,'owned process exit unconfirmed'
 def close(self):
  if self.h:k.CloseHandle(self.h);self.h=None
class Host:
 def __init__(self,exe,peer,case,profile,mode,env):
  cwd=case/'child-cwd';cwd.mkdir();self.rows=[];self.q=queue.Queue();self.case=case;self.errors=(case/'stderr.txt').open('xb')
  args=[str(exe),'serve','--profile',str(profile),'--slot','life007','--client',str(peer),'--sha256',sha(peer),'--work-dir',str(cwd),
   '--plugin-id','synthetic007','--role','qualification','--operation','life007','--http-origin','http://127.0.0.1:1',
   '--ttl-ms','8000','--close-ms','300','--client-arg',mode,'--client-arg',str(case/'peer.json')]
  dump(case/'launch.json',{'args':args,'environment':env})
  self.p=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.errors,env=env,cwd=case)
  def read():
   try:
    for line in self.p.stdout:
     v=json.loads(line);self.rows.append(v);self.q.put(v)
   except Exception as e:self.q.put({'reader_error':repr(e)})
   self.q.put(None)
  self.reader=threading.Thread(target=read,daemon=True);self.reader.start();self.wait(lambda v:v.get('event')=='proposal')
 def wait(self,pred,timeout=10):
  end=time.monotonic()+timeout
  while time.monotonic()<end:
   v=self.q.get(timeout=max(.01,end-time.monotonic()))
   if v is None:raise RuntimeError('host EOF before required observation')
   assert 'reader_error' not in v,v
   if pred(v):return v
  raise TimeoutError('required event missing')
 def cmd(self,action,expected=True,**fields):
  self.p.stdin.write((json.dumps({'action':action,**fields})+'\n').encode());self.p.stdin.flush()
  v=self.wait(lambda v:v.get('event')=='operator_result' and v.get('action')==action)
  assert v['ok']==expected,v
  return v.get('result',v)
 def done(self,terminate=False):
  if self.p.poll() is None and terminate:self.p.kill()
  rc=self.p.wait(timeout=5)
  self.reader.join(3)
  if self.reader.is_alive():raise RuntimeError('diagnostic reader join missing')
  self.p.stdin.close();self.p.stdout.close();self.errors.close();dump(self.case/'host-rows.json',self.rows)
  return rc
 def save(self):dump(self.case/'host-rows.json',self.rows)
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--check',type=Path,required=True);parser.add_argument('--out',type=Path,required=True);a=parser.parse_args()
 check=a.check.resolve();receipt=load(check/'receipt.json');assert receipt['source_unchanged'] and all(c['exit_code']==0 for c in receipt['commands'])
 out=a.out.resolve();out.relative_to(ROOT);out.mkdir(parents=True,exist_ok=False)
 candidate=out/'candidate';candidate.mkdir()
 for rel,h in receipt['sources_before'].items():
  assert sha(ROOT/rel)==h,rel
  d=candidate/'source'/rel;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/rel,d);assert sha(d)==h
 exe=candidate/'morrow-native-stream-host.exe';peer=candidate/'morrow-native-close-peer.exe'
 for p in [exe,peer]:shutil.copyfile(check/'default-build'/p.name,p)
 manifest={'sources':receipt['sources_before'],'executables':{p.name:sha(p) for p in [exe,peer]},'check_receipt_sha256':sha(check/'receipt.json'),'sdk_frozen':False,'runner_sha256':sha(Path(__file__))}
 dump(candidate/'manifest.json',manifest)
 env={key:os.environ[key] for key in ['SYSTEMROOT','WINDIR','COMSPEC'] if key in os.environ}
 env['LOCALAPPDATA']=os.environ['LOCALAPPDATA'];assert Path(env['LOCALAPPDATA']).is_dir()
 results=[]
 for mode in ['crash-owner','descendant-stdio']:
  case=out/mode;case.mkdir();profile=case/'profile';profile.mkdir();owned=[];hosts=[];r={'case':mode,'passed':False,'recovery_claimed':False,'isolation_claimed':False,'http_requests':0}
  try:
   init=subprocess.run([str(exe),'init','--profile',str(profile),'--slot','life007'],env=env,capture_output=True,timeout=10)
   (case/'init.stdout').write_bytes(init.stdout);(case/'init.stderr').write_bytes(init.stderr);assert init.returncode==0
   host=Host(exe,peer,case,profile,'expiry-no-close' if mode=='crash-owner' else mode,env);hosts.append(host)
   grant=host.cmd('approve')['grant_id'];claim=host.cmd('claim',grant_id=grant);child=OwnedProcess(claim['pid']);owned.append(child)
   if mode=='crash-owner':
    active=host.wait(lambda v:v.get('event')=='host_observation' and v['observation']['event']=='phase' and v['observation']['detail']=='Active')
    host.wait(lambda v:v.get('event')=='host_observation' and v['observation']['event']=='control_frame_sent' and v['observation']['ordinal']>active['observation']['ordinal'])
    before=host.cmd('inspect');dump(case/'before-crash.json',before);assert before['owner']['owner_retained']
    host.p.kill();host.p.wait(timeout=5)
    r['child_alive_immediately_after_host_exit']=child.alive()
    child.finish();r['exact_owned_child_exit_confirmed']=True;host.done()
    secondcase=case/'restart';secondcase.mkdir();restart=Host(exe,peer,secondcase,profile,'expiry-no-close',env);hosts.append(restart)
    state=restart.cmd('inspect',grant_id=grant);dump(case/'after-crash.json',state)
    assert state['owner']['owner_retained'] and state['owner']['phase']!='Released' and not state['owner']['locally_observed']
    assert state['grant']['live_in_this_issuer'] is False and state['owner']['automatic_takeover'] is False
    old=restart.cmd('claim',False,grant_id=grant);assert 'no live approval' in old['error']
    newgrant=restart.cmd('approve')['grant_id'];new=restart.cmd('claim',False,grant_id=newgrant)
    assert 'no automatic crash takeover' in new['error']
    finalstate=restart.cmd('inspect');assert finalstate['owner']==state['owner']
    restart.cmd('quit');restart.wait(lambda v:v.get('event')=='final_without_session');assert restart.done()==2
    r.update(persistent_owner_unchanged=True,old_grant_rejected=True,new_claim_rejected=True,owner_retained_after_actual_child_exit=True)
   else:
    host.wait(lambda v:v.get('event')=='host_observation' and v['observation']['event']=='phase' and v['observation']['detail']=='ClosingUnconfirmed')
    evidence=load(case/'peer.json');assert evidence['direct_pid']==claim['pid'] and evidence['inherited_stdio']
    descendant=OwnedProcess(evidence['descendant_pid']);owned.append(descendant);assert descendant.alive() and not child.alive()
    state=host.cmd('inspect');dump(case/'unconfirmed-before-test-cleanup.json',state)
    assert state['owner']['owner_retained'] and state['owner']['phase']=='ClosingUnconfirmed'
    assert state['owner']['exit_observed'] and not state['owner']['stdout_eof'] and not state['owner']['stderr_eof']
    descendant.finish();r['exact_owned_descendant_exit_confirmed']=True
    final=host.wait(lambda v:v.get('event')=='final');assert host.done()==2
    assert final['state']['owner']['phase']=='Released' and not final['state']['owner']['owner_retained']
    assert any(e['event']=='owner_retained' for e in final['snapshot']['events'])
    r.update(direct_exit_without_stdio_eof_rejected=True,unconfirmed_owner_before_external_test_cleanup=True,released_only_after_owned_descendant_cleanup=True)
   r['passed']=True
  except Exception as e:r['error']=repr(e)
  finally:
   for process in owned:
    try:process.finish();process.close()
    except Exception as e:r.setdefault('cleanup_errors',[]).append(repr(e))
   for host in hosts:
    try:
     if host.p.poll() is None:host.done(terminate=True)
     else:host.save()
    except Exception as e:r.setdefault('cleanup_errors',[]).append(repr(e))
   if r.get('cleanup_errors'):r['passed']=False
   dump(case/'result.json',r);results.append(r)
  if not r['passed']:break
 unchanged=sha(Path(__file__))==manifest['runner_sha256'] and all(sha(ROOT/rel)==h for rel,h in manifest['sources'].items())
 result={'status':'passed' if unchanged and len(results)==2 and all(r['passed'] for r in results) else 'failed','cases':results,'sources_unchanged':unchanged,'scope':'real Windows safety-refusal and inherited-stdio evidence; no trusted recovery or malicious same-user isolation qualification'}
 dump(out/'result.json',result)
 files={p.relative_to(out).as_posix():sha(p) for p in out.rglob('*') if p.is_file()}
 dump(out/'evidence-manifest.json',{'files':files,'scenario_replayed':False})
 print(json.dumps({'path':str(out),**result}));return 0 if result['status']=='passed' else 1
if __name__=='__main__':raise SystemExit(main())
