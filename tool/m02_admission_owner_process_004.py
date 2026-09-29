"""Real owned-host/process qualification. No arbitrary PID discovery or user data."""
import pathlib,json,hashlib,os,subprocess,threading,queue,time,datetime,shutil,ctypes
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m02-admission-owner-002'
HOST=ROOT/'target/m02-admission-owner-002/debug/morrow-native-owner-host.exe'
PLUGIN=pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
CLIENT=PLUGIN/'out/m02-native-session-001/candidates/client-001/morrow-session-client.exe'
FIXTURE=ROOT/'reports/codex-morrow-v1.1/host/m02-native-session-001/runtime-kit-001/bin/fixture-client.exe'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
RUN=BASE/('process-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));RUN.mkdir()
acl=subprocess.run(['powershell.exe','-NoProfile','-File',str(ROOT/'tool/m02_owner_private_dir_001.ps1'),'-Path',str(RUN)],capture_output=True,check=True,env={**{k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC'] if k in os.environ},'PSModulePath':r'C:\Windows\System32\WindowsPowerShell\v1.0\Modules'})
(RUN/'acl.txt').write_bytes(acl.stdout)
LOCAL=RUN/'local-app-data';LOCAL.mkdir()
ENV={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC'] if k in os.environ};ENV.update({'LOCALAPPDATA':str(LOCAL),'HOME':str(RUN),'USERPROFILE':str(RUN),'APPDATA':str(RUN),'TEMP':str(RUN),'TMP':str(RUN)})
REPORT={'status':'failed','host_sha256':sha(HOST),'ordinary_client_sha256':sha(CLIENT),'fixture_sha256':sha(FIXTURE),'cases':[]}
PROCS=[]
def direct(args):return subprocess.run([str(HOST)]+[str(x) for x in args],env=ENV,cwd=RUN,capture_output=True,timeout=8)
def profile(name):
 p=RUN/(name+'-profile');p.mkdir();r=direct(['init','--profile',p,'--slot','native-control']);assert r.returncode==0,r.stderr;return p
class Host:
 def __init__(self,name,p,client=CLIENT,extra=None,ttl=8000):
  self.name=name;self.rows=[];self.raw=[];self.q=queue.Queue();self.err=[]
  cwd=RUN/(name+'-cwd');cwd.mkdir()
  args=[str(HOST),'serve','--profile',str(p),'--client',str(client),'--sha256',sha(client),'--work-dir',str(cwd),'--plugin-id','test-plugin','--role','reader','--operation','test-operation','--ttl-ms',str(ttl)]
  for value in (extra if extra is not None else ['--queries','16','--interval-ms','150']):args+=['--client-arg',value]
  self.p=subprocess.Popen(args,cwd=RUN,env=ENV,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);PROCS.append(self)
  def reader():
   for line in self.p.stdout:self.q.put(line)
   self.q.put(None)
  def errors():self.err.append(self.p.stderr.read())
  self.reader=threading.Thread(target=reader,daemon=True);self.reader.start();self.errors=threading.Thread(target=errors,daemon=True);self.errors.start();self.wait(lambda r:r.get('event')=='proposal')
 def wait(self,predicate,timeout=8,start=0):
  for r in self.rows[start:]:
   if predicate(r):return r
  end=time.monotonic()+timeout
  while time.monotonic()<end:
   line=self.q.get(timeout=max(.01,end-time.monotonic()))
   if line is None:raise RuntimeError(self.name+' exited before expected event: '+repr(self.rows[-2:]))
   self.raw.append(line);row=json.loads(line);self.rows.append(row)
   if predicate(row):return row
  raise TimeoutError(self.name)
 def command(self,action,id=None,expect=True,**fields):
  start=len(self.rows);value={'action':action,**fields}
  if id is not None:value['grant_id']=id
  self.p.stdin.write((json.dumps(value)+'\n').encode());self.p.stdin.flush()
  row=self.wait(lambda r:r.get('event')=='operator_result' and r.get('action')==action,start=start)
  assert row['ok']==expect,(self.name,row)
  return row.get('result',row)
 def approve(self):return self.command('approve')['grant_id']
 def start(self):i=self.approve();s=self.command('claim',i);return i,s
 def observation(self,event):return self.wait(lambda r:r.get('event')=='host_observation' and r['observation']['event']==event)
 def phase(self,phase):return self.wait(lambda r:r.get('event')=='host_observation' and r['observation']['event']=='phase' and r['observation']['detail']==phase)
 def finish(self,code=0):
  actual=self.p.wait(timeout=9);assert actual==code,(self.name,actual,self.rows[-2:]);self.p.stdin.close();self.reader.join(1);self.errors.join(1)
  while not self.q.empty():
   line=self.q.get()
   if line is not None:self.raw.append(line);self.rows.append(json.loads(line))
  self.save();return self.rows[-1]
 def save(self):
  (RUN/(self.name+'.stdout.jsonl')).write_bytes(b''.join(self.raw));(RUN/(self.name+'.stderr')).write_bytes(b''.join(self.err))
def passed(name,**detail):REPORT['cases'].append({'case':name,'passed':True,**detail})
def assert_release(row):
 s=row['snapshot'];assert s['phase']=='Released' and s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'] and not s['owner_retained'];assert row['state']['owner']['phase']=='Released' and not row['state']['owner']['owner_retained'];return s
try:
 # Simultaneous init cannot generate two distinct identities.
 p=RUN/'init-race-profile';p.mkdir();args=[str(HOST),'init','--profile',str(p),'--slot','native-control']
 init=[subprocess.Popen(args,cwd=RUN,env=ENV,stdout=subprocess.PIPE,stderr=subprocess.PIPE)for _ in range(2)]
 outputs=[x.communicate(timeout=8)for x in init];codes=[x.returncode for x in init];assert sorted(codes)==[0,2];passed('two_initializer_processes',exit_codes=codes)
 # Two real hosts approve separately, then race for one profile owner.
 a=Host('race-a',p);b=Host('race-b',p);ia=a.approve();ib=b.approve()
 barrier=threading.Barrier(2);results={}
 def claim(h,i):barrier.wait();results[h.name]=h.command('claim',i,expect=None)
 # Dispatch both requests before reading either result, no sequential claim shortcut.
 for h,i in [(a,ia),(b,ib)]:h.p.stdin.write((json.dumps({'action':'claim','grant_id':i})+'\n').encode());h.p.stdin.flush()
 ar=a.wait(lambda r:r.get('action')=='claim');br=b.wait(lambda r:r.get('action')=='claim');assert int(ar['ok'])+int(br['ok'])==1
 winner,loser=(a,b)if ar['ok']else(b,a);winid=ia if winner is a else ib
 winner.phase('Active');loser.command('inspect');loser.command('quit');loser.finish(2);winner.command('stop');first=assert_release(winner.finish());passed('two_hosts_same_slot_live_reject',winner_pid=winner.p.pid,child_pid=first['pid'],loser_pid=loser.p.pid)
 c=Host('after-release',p,extra=['--queries','1']);ic,cs=c.start();final=assert_release(c.finish());assert final['epoch']>first['epoch'];passed('release_allows_new_grant_generation',first_epoch=first['epoch'],new_epoch=final['epoch'],new_id=ic)
 # Delay after authorization never renews the initial deadline.
 d=Host('delayed-claim',profile('delayed'),ttl=150);di=d.approve();time.sleep(.22);rejection=d.command('claim',di,expect=False);assert 'expired'in rejection['error'];d.command('quit');d.finish(2);passed('delayed_claim_original_expiry')
 # Revoked old reference denies; new explicit authorization is distinct.
 e=Host('revoke-before',profile('revoke-before'),extra=['--queries','1']);old=e.approve();e.command('revoke',old);e.command('claim',old,expect=False);new=e.approve();assert new!=old;e.command('claim',new);assert_release(e.finish(2));passed('revoked_old_grant_new_explicit_identity',old=old,new=new)
 # Durable ID from a terminated issuer cannot create an in-memory approval.
 p=profile('restart');f=Host('issuer-old',p);fi=f.approve();f.command('quit');f.finish();g=Host('issuer-new',p);g.command('claim',fi,expect=False);g.command('quit');g.finish(2);passed('historical_grant_new_issuer_rejected')
 # Unknown guest-like approval flags on operator data are not accepted.
 p=profile('unapproved');u=Host('unapproved',p);u.command('claim','0'*64,expect=False,approved=True);u.command('quit');u.finish(2);passed('approved_data_flag_has_no_authority')
 # External host commits revocation, owning host applies it to real supervisor.
 p=profile('external-revoke');a=Host('external-owner',p);ai,_=a.start();a.observation('state_read');b=Host('external-controller',p);r=b.command('revoke',ai);assert r['persisted'] and not r['runtime_applied'];b.command('inspect',ai);b.command('quit');b.finish();s=assert_release(a.finish());assert s['exit_code']==19;assert any(r.get('observation',{}).get('event')=='external_revocation_applied'for r in a.rows);passed('external_revoke_persisted_then_runtime_applied',child_exit=s['exit_code'])
 # Parent exit cannot release owner while inherited output handles remain open.
 p=profile('held-output');a=Host('output-holder',p,FIXTURE,['hold-output']);a.start();a.phase('ClosingUnconfirmed');b=Host('output-contender',p);bi=b.approve();b.command('claim',bi,expect=False);seen=b.command('inspect');assert seen['owner']['owner_retained'];b.command('quit');b.finish(2);s=assert_release(a.finish(2));passed('closing_unconfirmed_blocks_second_host',child_pid=s['pid'])
 # Kill only our own host, keep a direct handle to its known test child.
 p=profile('crash');a=Host('crash-owner',p,FIXTURE,['ignore-stop']);ai,spawn=a.start();a.phase('Active');pid=spawn['pid']
 kernel=ctypes.WinDLL('kernel32',use_last_error=True);kernel.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.c_uint32];kernel.OpenProcess.restype=ctypes.c_void_p;kernel.WaitForSingleObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32];kernel.CloseHandle.argtypes=[ctypes.c_void_p]
 handle=kernel.OpenProcess(0x00100000|0x1000,False,pid);assert handle
 a.p.kill();a.p.wait(timeout=5);assert kernel.WaitForSingleObject(handle,0)==258
 b=Host('crash-contender',p);bi=b.approve();denied=b.command('claim',bi,expect=False);assert 'unconfirmed'in denied['error'];state=b.command('inspect');assert state['owner']['pid']==pid and state['owner']['owner_retained'];b.command('quit');b.finish(2)
 assert kernel.WaitForSingleObject(handle,6500)==0;kernel.CloseHandle(handle);a.reader.join(1);a.errors.join(1)
 while not a.q.empty():
  line=a.q.get()
  if line is not None:a.raw.append(line);a.rows.append(json.loads(line))
 a.save();a.p.stdin.close()
 c=Host('crash-after-child-exit',p);ci=c.approve();c.command('claim',ci,expect=False);c.command('quit');c.finish(2);passed('host_crash_live_child_no_takeover_and_no_auto_recovery',host_pid=a.p.pid,child_pid=pid,child_alive_after_host_kill=True)
 # Two real processes concurrently claim and revoke the same original grant.
 q=profile('claim-revoke-race');x=Host('claim-racer',q);y=Host('revoke-racer',q);xi=x.approve()
 for h,message in [(x,{'action':'claim','grant_id':xi}),(y,{'action':'revoke','grant_id':xi})]:h.p.stdin.write((json.dumps(message)+'\n').encode());h.p.stdin.flush()
 xr=x.wait(lambda r:r.get('action')=='claim');yr=y.wait(lambda r:r.get('action')=='revoke');assert yr['ok'];y.command('quit');y.finish()
 if xr['ok']:
  xs=assert_release(x.finish());assert xs['exit_code']==19
 else:
  x.command('quit');x.finish(2);assert not any(r.get('event')=='host_observation' for r in x.rows)
 passed('two_process_claim_revoke_race',claim_accepted=xr['ok'],revoke_persisted=yr['result']['persisted'])
 # Profile copy carries a stale canonical root and must not become a split journal.
 copied=RUN/'copied-profile';shutil.copytree(p,copied);r=direct(['inspect','--profile',copied]);assert r.returncode==2 and b'location'in r.stderr;passed('copied_profile_rejected')
 REPORT['status']='passed_real_multi_host_authority_owner_slice'
except Exception as e:REPORT['error']=repr(e)
finally:
 for h in PROCS:
  if h.p.poll() is None:
   try:h.p.stdin.write(b'{"action":"stop"}\n{"action":"quit"}\n');h.p.stdin.flush();h.p.wait(timeout=10)
   except Exception as e:REPORT.setdefault('cleanup_unconfirmed',[]).append({'host_pid':h.p.pid,'error':repr(e)})
  h.save()
 (RUN/'result.json').write_text(json.dumps(REPORT,indent=2)+'\n')
 print(json.dumps({'status':REPORT['status'],'case_count':len(REPORT['cases']),'path':str(RUN/'result.json'),'error':REPORT.get('error')}))
raise SystemExit(0 if REPORT['status'].startswith('passed') else 1)


