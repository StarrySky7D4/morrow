"""Pure PowerShell round-reader negative tests, using only public closed data."""
from pathlib import Path
import copy,hashlib,json,os,subprocess
BASE=Path(__file__).parent
STAGE=Path(json.loads((BASE/'vm-stage-selection-042.json').read_bytes())['root'])
OUT=BASE/'shared-guest-reader-042-pure';OUT.mkdir()
def save(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2)
old=json.loads((BASE/'c28-root-vm-transaction2769-cb270f78ca714c37b177b055ec35d56c-case/case-terminal.original.json').read_bytes())
guest=(STAGE/'Guest-Transaction1737.ps1').read_text(encoding='utf-8-sig')
start=guest.index('function CheckRound042(');end=guest.index('function CheckTerminal(',start)
function=guest[start:end]
rounds=[]
for n in range(1,18):
    frames=copy.deepcopy(old['original_helper']['input']['frames'])
    for f in frames:f['ordinal']+=(n-1)*92
    closed=old['steps'][-1]['returned']+1
    rounds.append(dict(schema='ORIGINAL_SHARED_PARENT_ROUND042_V1',attempt=n,
        steps=old['steps'],frames=frames,first_line=(n-1)*92+1,last_line=n*92,input_bytes=3557,
        same_original_parent_pid=old['original_helper']['pid'],same_original_parent_creation=old['original_helper']['process_creation_filetime'],
        parent_lifetime_closed=False,source_accepted=False,release_eligible=False,
        original_attempt_closed_sequence=closed,original_common_owner_closed_sequence=closed+1 if n==17 else -1,
        prompt_native_write_baseline=0,journal_records=closed+2 if n==17 else closed+1))
save(OUT/'fixtures.json',dict(helper=old['original_helper'],rounds=rounds,synthetic_pure_test_data=True))
script=r'''$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
function N([bool]$b,[string]$why){if(-not $b){throw $why}}
function Pass([string]$name){$script:count++;Write-Output ('PASS '+$name)}
function Reject([scriptblock]$action,[string]$name){$caught=$false;try{& $action}catch{$caught=$true};N $caught ('NOT_REJECTED_'+$name);Pass $name}
'''+function+r'''
$script:count=0
$f=Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'fixtures.json') | ConvertFrom-Json
for($i=0;$i-lt 17;$i++){CheckRound042 $f.rounds[$i] ($i+1) $f.helper;Pass ('exact_round_'+($i+1))}
function Fresh([int]$i){return ((Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'fixtures.json')|ConvertFrom-Json).rounds[$i])}
$r=Fresh 1;$r.attempt=1;Reject {CheckRound042 $r 2 $f.helper} 'prior_round_substitution'
$r=Fresh 1;$r.frames[0].ordinal=1;Reject {CheckRound042 $r 2 $f.helper} 'prior_input_history_substitution'
$r=Fresh 1;$r.same_original_parent_pid++;Reject {CheckRound042 $r 2 $f.helper} 'different_parent_pid'
$r=Fresh 1;$r.same_original_parent_creation++;Reject {CheckRound042 $r 2 $f.helper} 'reused_pid_different_creation'
$r=Fresh 1;$r.frames[0].native_returned=$false;Reject {CheckRound042 $r 2 $f.helper} 'unreturned_input'
$r=Fresh 1;$r.frames[0].raw_written=1;Reject {CheckRound042 $r 2 $f.helper} 'partial_native_input'
$r=Fresh 1;$r.steps[40].command='sealed-basic-observe';Reject {CheckRound042 $r 2 $f.helper} 'normal_business_is_not_refusal'
$r=Fresh 1;$r.original_attempt_closed_sequence=$r.steps[46].returned;Reject {CheckRound042 $r 2 $f.helper} 'exit_is_not_original_attempt_retirement'
$r=Fresh 1;$r.original_common_owner_closed_sequence=$r.original_attempt_closed_sequence+1;Reject {CheckRound042 $r 2 $f.helper} 'premature_common_owner_finish'
$r=Fresh 16;$r.original_common_owner_closed_sequence=-1;Reject {CheckRound042 $r 17 $f.helper} 'missing_final_common_owner_finish'
$r=Fresh 16;$r.parent_lifetime_closed=$true;Reject {CheckRound042 $r 17 $f.helper} 'round_report_cannot_synthesize_parent_close'
N ($script:count-eq 28) 'EXACT_PURE_COUNT'
Write-Output ('GUEST_ROUND_PURE_PASSED='+$script:count)
'''
(OUT/'pure.ps1').write_text(script,encoding='utf-8-sig')
cmd=['powershell.exe','-NoProfile','-NonInteractive','-File',str(OUT/'pure.ps1')]
run=subprocess.run(cmd,capture_output=True,env=dict(os.environ,TEMP=str(OUT),TMP=str(OUT)),timeout=60)
(OUT/'stdout.log').write_bytes(run.stdout);(OUT/'stderr.log').write_bytes(run.stderr)
assert run.returncode==0 and not run.stderr,run.stderr.decode(errors='replace')
assert run.stdout.decode().splitlines()[-1]=='GUEST_ROUND_PURE_PASSED=28'
save(OUT/'result.json',dict(status='PASS_PURE_GUEST_ROUND_READER_NOT_RUNTIME',methods=28,exit_code=0,
    exact_function_sha256=hashlib.sha256(function.encode()).hexdigest(),stage_guest_sha256=hashlib.sha256((STAGE/'Guest-Transaction1737.ps1').read_bytes()).hexdigest(),VM_calls=0,product_executions=0))
print('PASS: 28 pure round-reader checks; no VM or product invocation.')
