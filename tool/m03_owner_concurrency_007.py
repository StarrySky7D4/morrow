"""Real local multi-host ownership and control cleanup. No HTTP or recovery takeover."""
from pathlib import Path
import argparse,importlib.util,json,os,shutil,subprocess
ROOT=Path(__file__).resolve().parents[1]
loader=importlib.util.spec_from_file_location('process007',ROOT/'tool/m03_lifecycle_process_007.py');m=importlib.util.module_from_spec(loader);loader.loader.exec_module(m)
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--candidate',type=Path,required=True);p.add_argument('--sha256',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
 candidate=a.candidate.resolve();assert m.sha(candidate/'manifest.json')==a.sha256
 manifest=m.load(candidate/'manifest.json')
 for rel,h in manifest['sources'].items():assert m.sha(candidate/'source'/rel)==h and m.sha(ROOT/rel)==h
 for rel,h in manifest['executables'].items():assert m.sha(candidate/rel)==h
 out=a.out.resolve();out.relative_to(ROOT);out.mkdir(parents=True,exist_ok=False)
 shutil.copyfile(Path(__file__),out/'runner.py');shutil.copyfile(ROOT/'tool/m03_lifecycle_process_007.py',out/'process-helper.py')
 hashes={p.name:m.sha(p) for p in [out/'runner.py',out/'process-helper.py']}
 exe=candidate/'morrow-native-stream-host.exe';peer=candidate/'morrow-native-close-peer.exe'
 env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC'] if k in os.environ};env['LOCALAPPDATA']=os.environ['LOCALAPPDATA'];assert Path(env['LOCALAPPDATA']).is_dir()
 hosts=[];handles=[];r={'status':'failed','http_requests':0,'sdk_frozen':False,'candidate_manifest_sha256':a.sha256,'cases':[]}
 try:
  profiles=[]
  for i in range(4):
   case=out/f'session-{i}';case.mkdir();profile=case/'profile';profile.mkdir();profiles.append(profile)
   init=subprocess.run([str(exe),'init','--profile',str(profile),'--slot','life007'],env=env,capture_output=True,timeout=10)
   (case/'init.stdout').write_bytes(init.stdout);(case/'init.stderr').write_bytes(init.stderr);assert init.returncode==0
   host=m.Host(exe,peer,case,profile,'expiry-normal',env);hosts.append(host)
   grant=host.cmd('approve')['grant_id'];claim=host.cmd('claim',grant_id=grant);h=m.OwnedProcess(claim['pid']);handles.append(h)
   host.wait(lambda v:v.get('event')=='host_observation' and v['observation']['event']=='phase' and v['observation']['detail']=='Active')
  # All four actual children must still overlap in time before any expiry/cleanup.
  assert all(h.alive() for h in handles),'four live sessions did not overlap'
  contendercase=out/'same-profile-contender';contendercase.mkdir()
  contender=m.Host(exe,peer,contendercase,profiles[0],'expiry-normal',env);hosts.append(contender)
  grant=contender.cmd('approve')['grant_id'];rejection=contender.cmd('claim',False,grant_id=grant)
  assert 'owner busy or unavailable' in rejection['error'],rejection
  assert not any(v.get('action')=='claim' and v.get('ok') for v in contender.rows)
  contender.cmd('quit');contender.wait(lambda v:v.get('event')=='final_without_session');assert contender.done()==2
  r['cases'].append({'case':'same-profile-live-owner','passed':True,'error':rejection['error'],'second_child_started':False})
  for i,host in enumerate(hosts[:4]):
   final=host.wait(lambda v:v.get('event')=='final',timeout=12);assert host.done()==0
   s=final['snapshot'];progress=s['http']['progress'];owner=final['state']['owner']
   assert final['expired_terminal_protocol_complete'] and owner['phase']=='Released' and not owner['owner_retained']
   assert s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'] and not s['owner_retained'] and s['event_overflow']==0
   assert progress['request_closed'] and progress['data_closed'] and progress['connect_reaped'] and not progress['worker_started']
   assert not handles[i].alive()
   r['cases'].append({'case':f'concurrent-session-{i}','passed':True,'child_pid':handles[i].pid,'host_exit':0,'owner_released':True,'no_http_worker':True,'terminal_protocol_complete':True})
  assert all(m.sha(ROOT/rel)==h for rel,h in manifest['sources'].items())
  r.update(status='passed',four_actual_sessions_overlapped=True,all_owned_children_exited=True,source_unchanged=True,runner_hashes=hashes)
 except Exception as e:r['error']=repr(e)
 finally:
  for handle in handles:
   try:handle.finish();handle.close()
   except Exception as e:r.setdefault('cleanup_errors',[]).append(repr(e))
  for host in hosts:
   try:
    if host.p.poll() is None:host.done(terminate=True)
    else:host.save()
   except Exception as e:r.setdefault('cleanup_errors',[]).append(repr(e))
  if r.get('cleanup_errors'):r['status']='failed'
  m.dump(out/'result.json',r)
  m.dump(out/'evidence-manifest.json',{'files':{f.relative_to(out).as_posix():m.sha(f) for f in out.rglob('*') if f.is_file()},'scope':'four real control-only concurrent sessions plus one same-profile refusal; no scalability, hostile-user isolation or recovery claim'})
 print(json.dumps({'path':str(out),**r}));return 0 if r['status']=='passed' else 1
if __name__=='__main__':raise SystemExit(main())
