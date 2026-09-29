"""Prove a rejected real child has a nonzero host outcome after verified release."""
import pathlib,hashlib,json,os,subprocess,datetime
ROOT=pathlib.Path(__file__).resolve().parents[1]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
run=ROOT/'reports/codex-morrow-v1.1/host/m02-native-session-001'/('negative-cli-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'))
run.mkdir();cwd=run/'cwd';cwd.mkdir()
host=ROOT/'target/m02-native-session-001/debug/morrow-native-session-host.exe'
fixture=ROOT/'target/m02-native-session-001/debug/examples/fixture-client.exe'
env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC'] if k in os.environ}
argv=[str(host),'--client',str(fixture),'--sha256',sha(fixture),'--work-dir',str(cwd),'--client-arg','malformed']
with (run/'stdout.jsonl').open('wb') as out,(run/'stderr.txt').open('wb') as err:
    proc=subprocess.Popen(argv,cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=out,stderr=err)
    try: code=proc.wait(timeout=8)
    finally: proc.stdin.close()
rows=[json.loads(v) for v in (run/'stdout.jsonl').read_text().splitlines()]
last=rows[-1];s=last['snapshot']
passed=code==2 and last['session_outcome']=='rejected_or_disconnected' and s['phase']=='Released' and s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'] and not s['owner_retained'] and s['event_overflow']==0
result={'passed':passed,'host_exit':code,'host_sha256':sha(host),'client_sha256':sha(fixture),'argv':argv,'final':last}
(run/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'passed':passed,'path':str(run/'result.json')}))
raise SystemExit(0 if passed else 1)
