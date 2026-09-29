"""Reuse exact real-process harness definitions; run frozen separate historical peer."""
import pathlib
helper=pathlib.Path(__file__).with_name('m02_admission_owner_process_004.py')
prefix=helper.read_text(encoding='utf-8-sig').split('\ntry:\n # Simultaneous init')[0]
exec(compile(prefix.replace("BASE/('process-'","BASE/('peer-'"),str(helper),'exec'))
MANIFEST=PLUGIN/'receipts/m02-admission-owner-002/candidate-001.json'
assert sha(MANIFEST)=='8851b4b05d3f1fe5ffec21bcfe8eeaf67f35b55573e9e95a12b45a25f8e396b5'
candidate=json.loads(MANIFEST.read_text())
PEER=pathlib.Path(candidate['exe'])
def inputs():
 actual={name:sha(PLUGIN/name) for name in candidate['input_sha256']};assert actual==candidate['input_sha256'];return actual
REPORT['plugin_inputs_before']=inputs();REPORT['peer_sha256']=sha(PEER);REPORT['peer_manifest_sha256']=sha(MANIFEST)
try:
 prep=subprocess.run(['powershell.exe','-NoProfile','-File',str(PLUGIN/'receipts/m02-admission-owner-002/prepare_materials.ps1')],capture_output=True,check=True,env={**{k:os.environ[k]for k in ['SYSTEMROOT','WINDIR','COMSPEC']if k in os.environ},'PSModulePath':r'C:\Windows\System32\WindowsPowerShell\v1.0\Modules'})
 materials=json.loads(prep.stdout);(RUN/'materials.json').write_text(json.dumps(materials,indent=2));REPORT['materials']=materials
 p=profile('historical-wire');source=None
 for scenario in ['capture','replay-hello','replay-query']:
  extra=['--scenario',scenario,'--evidence-dir',materials['paths'][scenario]]
  if scenario!='capture':extra+=['--capture-dir',materials['paths']['capture']]
  h=Host(scenario,p,PEER,extra);gid,spawn=h.start();last=h.finish(0 if scenario=='capture'else 2);snap=assert_release(last)
  assert snap['exit_code']==(0 if scenario=='capture'else 17),(scenario,snap['exit_code'])
  received=[bytes.fromhex(e['detail']['raw_hex'])for e in snap['events']if e['event']=='frame_received']
  if scenario=='capture':
   assert len(received)>=3;source=received;assert (pathlib.Path(materials['paths']['capture'])/'complete.txt').is_file()
  elif scenario=='replay-hello':assert received[0]==source[0]
  else:assert received[1]==source[1] and received[0]!=source[0]
  passed(scenario,grant_id=gid,epoch=snap['epoch'],child_pid=snap['pid'],child_exit=snap['exit_code'],historical_bytes_exact=scenario!='capture')
 # New plugin-owned holder is optional complementary producer evidence, separate from fixture.
 h=Host('peer-held-output',p,PEER,['--scenario','hold-output','--evidence-dir',materials['paths']['hold-output']]);h.start();h.phase('ClosingUnconfirmed');last=h.finish();snap=assert_release(last)
 holder=json.loads((pathlib.Path(materials['paths']['hold-output'])/'holder.json').read_text());assert holder['root_pid']==snap['pid'];passed('peer-held-output',child_pid=snap['pid'],holder_pid=holder['holder_pid'],holder_identity_source='peer self-reported, no independent OS assertion in producer case')
 REPORT['plugin_inputs_after']=inputs();assert REPORT['plugin_inputs_before']==REPORT['plugin_inputs_after'];REPORT['status']='passed_real_historical_peer_cases'
except Exception as e:REPORT['error']=repr(e)
finally:
 for h in PROCS:
  if h.p.poll()is None:
   try:h.p.stdin.write(b'{"action":"stop"}\n{"action":"quit"}\n');h.p.stdin.flush();h.p.wait(timeout=10)
   except Exception as e:REPORT.setdefault('cleanup_unconfirmed',[]).append({'pid':h.p.pid,'error':repr(e)})
  h.save()
 (RUN/'result.json').write_text(json.dumps(REPORT,indent=2)+'\n')
 print(json.dumps({'status':REPORT['status'],'case_count':len(REPORT['cases']),'path':str(RUN/'result.json'),'error':REPORT.get('error')}))
raise SystemExit(0 if REPORT['status'].startswith('passed')else 1)
