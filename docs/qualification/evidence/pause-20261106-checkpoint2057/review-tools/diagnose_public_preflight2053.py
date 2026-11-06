"""Reproduce only public pin checks, without any VM/private/policy dispatch code."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
B=Path(__file__).parent
O=B/'public-preflight-diagnostic-2053'
selection=json.loads((B/'vm-stage-selection-2050.json').read_bytes())
S=Path(selection['root'])
def pin(p):
    raw=p.read_bytes();return dict(path=str(p),bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
def save(p,value):
    with p.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2)
assert pin(Path(selection['script']['path']))==selection['script']
assert pin(Path(selection['plan']['path']))==selection['plan']
original=Path(selection['script']['path']).read_text(encoding='utf-8-sig')
boundary=' # ROOT MUST have a real human answer before selecting apply-run-restore.'
assert original.count(boundary)==1
prefix=original.split(boundary)[0]
assert 'New-PSSession' not in prefix and 'Import-Clixml' not in prefix
assert '$vm=GuardHost1737' not in prefix and 'Guest1737 ' not in prefix
prefix=prefix.replace("# SOURCE1737.","if($Mode-cne 'public-only'){exit 78}\n# SOURCE1737.",1)
tail=r'''
 throw 'DIAGNOSTIC_PUBLIC_BOUNDARY_ONLY'
}catch{
 $row=@{code=[string]$_.FullyQualifiedErrorId;message=[string]$_.Exception.Message;script=[string]$_.InvocationInfo.ScriptName;line=$_.InvocationInfo.ScriptLineNumber;stack=[string]$_.ScriptStackTrace;vm_connection_attempted=$false}
 [Console]::Error.WriteLine(($row|ConvertTo-Json -Compress))
 foreach($f in $Root1737.PublicHolds){if($f.CanRead){$f.Dispose()}}
 exit 78
}
'''
O.mkdir()
diagnostic=S/'PublicPreflightDiagnostic2053.ps1'
with diagnostic.open('xb') as f:f.write((prefix+tail).encode('utf-8'))
save(O/'derivation.json',dict(original=selection['script'],diagnostic=pin(diagnostic),only_public_prefix=True,all_vm_private_policy_dispatch_removed=True,common=pin(S/'Common1737.ps1'),plan=selection['plan']))
ps=r'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
row=pin(diagnostic)
cmd=[ps,'-NoProfile','-NonInteractive','-File',str(diagnostic),'-Mode','public-only','-PlanPhase','stage-and-bind','-SelfBytes',str(row['bytes']),'-SelfSha256',row['sha256'],'-PlanBytes',str(selection['plan']['bytes']),'-PlanSha256',selection['plan']['sha256']]
env=dict(os.environ,PYTHONUTF8='1',PYTHONDONTWRITEBYTECODE='1',TEMP='D:/build_cache/tmp',TMP='D:/build_cache/tmp')
result=subprocess.run(cmd,capture_output=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
for leaf,raw in [('stdout',result.stdout),('stderr',result.stderr)]:
    with (O/leaf).open('xb') as f:f.write(raw)
save(O/'result.json',dict(status='PUBLIC_ONLY_DIAGNOSTIC_ORIGINAL_PROCESS_RETURNED',exit_code=result.returncode,stdout=pin(O/'stdout'),stderr=pin(O/'stderr'),diagnostic=pin(diagnostic),vm_calls=0,private_reads=0,policy_mutations=0,original_preflight_exit_code='NOT_RECORDED_NOT_INFERRED'))
print(result.stdout.decode('utf-8','replace'))
print(result.stderr.decode('utf-8','replace'))
print(json.dumps(dict(diagnostic_exit_code=result.returncode,report=str(O/'result.json'))))
