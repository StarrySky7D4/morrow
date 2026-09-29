"""Execute only the four fixed platform tests in a new isolated directory."""
import ctypes,hashlib,json,os,re,shutil,subprocess,time
from ctypes import wintypes
from pathlib import Path
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
HOST=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001';KIT=HOST/'pipe-kit-001'
RUN=HERE/'pipe-independent-001'
def sha(p):
    with open(p,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def save(p,x):p.write_text(json.dumps(x,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def identity(proc):
    k=ctypes.WinDLL('kernel32',use_last_error=True);h=wintypes.HANDLE(int(proc._handle))
    q=k.QueryFullProcessImageNameW;q.argtypes=[wintypes.HANDLE,wintypes.DWORD,wintypes.LPWSTR,ctypes.POINTER(wintypes.DWORD)];q.restype=wintypes.BOOL
    buf=ctypes.create_unicode_buffer(32768);n=wintypes.DWORD(len(buf))
    if not q(h,0,buf,ctypes.byref(n)):raise ctypes.WinError(ctypes.get_last_error())
    g=k.GetProcessTimes;g.argtypes=[wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4;g.restype=wintypes.BOOL
    t=[wintypes.FILETIME() for _ in range(4)]
    if not g(h,*(ctypes.byref(x) for x in t)):raise ctypes.WinError(ctypes.get_last_error())
    return {'pid':proc.pid,'image':buf.value,'image_sha256':sha(buf.value),'creation_filetime':(t[0].dwHighDateTime<<32)|t[0].dwLowDateTime,'source':'held exact Popen process handle'}
assert not RUN.exists();RUN.mkdir()
expected={HOST/'pipe-handoff-001.json':'a7949c64c87319a77376a75d2535d7bf1b4bc8ad9fc9ca63c6b73b0304063600',KIT/'manifest.json':'d6eee6bdb2f933d59ebeddb10a550359204cb024fe09e58f98182739bbd1787b',KIT/'result.json':'3e9dbe7efb7a75f704fbfb8ecf85dd966b088a808f3c8cc90d03864ee84287dd'}
for p,d in expected.items():assert sha(p)==d
manifest=read(KIT/'manifest.json');receipt=read(KIT/'result.json')
for rel,d in manifest['files'].items():expected[KIT/rel]=d
for rel,d in manifest['source_files'].items():expected[ROOT/'native_pipe_win_001'/rel]=d
for rel,d in receipt['before'].items():expected[ROOT/rel]=d
assert receipt['before']==receipt['after']
for rel,d in receipt['artifacts'].items():expected[ROOT/rel]=d
def pin(label):
    actual={str(p):sha(p) for p in expected};bad=[str(p) for p,d in expected.items() if actual[str(p)]!=d]
    save(RUN/f'identities-{label}.json',{'hashes':actual,'mismatches':bad});assert not bad,bad
pin('before')
messages=[json.loads(line) for line in (KIT/'compile.stdout').read_text().splitlines() if line.startswith('{')]
artifacts=[x for x in messages if x.get('reason')=='compiler-artifact']
target=next(x for x in artifacts if x['target']['name']=='platform' and x.get('executable'))
exe_hash='c51ba2c7de38ba636a78560d7277ceeff879dd428626195c4fe9ac3249a40467'
assert sha(target['executable'])==exe_hash
save(RUN/'producer-compiler-association.json',target)
for command in receipt['commands']:
    for stream in ['stdout','stderr']:assert sha(KIT/(command['name']+'.'+stream))==command[stream+'_sha256']
exe=RUN/'platform-a95421a369e34f9f.exe';shutil.copyfile(KIT/'tests'/exe.name,exe);assert sha(exe)==exe_hash
names=re.findall(r'#\[test\]\s*fn (\w+)\(', (KIT/'source/tests/platform.rs').read_text())
assert names.count('helper')==1;names.remove('helper');assert len(names)==4
env={k:os.environ[k] for k in ['SystemRoot','WINDIR','COMSPEC'] if k in os.environ}
for k,d in [('USERPROFILE','profile'),('APPDATA','profile/roaming'),('LOCALAPPDATA','profile/local'),('TEMP','tmp'),('TMP','tmp')]:
    p=RUN/d;p.mkdir(parents=True,exist_ok=True);env[k]=str(p)
env['PATH']=str(Path(env['SystemRoot'])/'System32')
save(RUN/'reviewer-inputs.json',{'runner_sha256':sha(__file__),'test_names':names,'environment':env,'exe_sha256':exe_hash})
results=[]
for i,name in enumerate(names,1):
    folder=RUN/f'case-{i:02d}';folder.mkdir();args=[str(exe),'--exact',name,'--nocapture','--test-threads=1']
    start=time.monotonic_ns()
    with open(folder/'stdout.txt','wb') as stdout,open(folder/'stderr.txt','wb') as stderr:
        proc=subprocess.Popen(args,cwd=folder,env=env,stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr,creationflags=subprocess.CREATE_NO_WINDOW)
        known=identity(proc);timedout=False
        try:code=proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            timedout=True;proc.kill();code=proc.wait(timeout=5)
            # Helper belongs to this test and has its own fixed 10s watchdog; no enumeration.
    output=(folder/'stdout.txt').read_text(encoding='utf-8')
    passed=code==0 and '1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out' in output
    case={'test':name,'argv':args,'process':known,'exit_code':code,'timed_out':timedout,'elapsed_ns':time.monotonic_ns()-start,'passed':passed,'stdout_sha256':sha(folder/'stdout.txt'),'stderr_sha256':sha(folder/'stderr.txt'),'helper_pids_reported_by_fixed_test':[int(x) for x in re.findall(r'child_pid=(\d+)',output)]}
    save(folder/'result.json',case);results.append(case);print(json.dumps({'test':name,'passed':passed,'exit_code':code}),flush=True)
pin('after');assert sha(exe)==exe_hash
result={'status':'independent_fixed_platform_tests_passed' if all(x['passed'] for x in results) else 'failed','substantive_passed':sum(x['passed'] for x in results),'substantive_total':4,'helper_entries_counted_as_tests':0,'results':results,'verified_input_count':len(expected),'independent_build':False,'binding':'producer actual compiler-artifact JSON plus source/lock pre-post and artifact hash, independent copied-exe execution','producer_frozen_count_not_independently_rehashed':receipt['frozen_count'],'limitations':['fixed producer fixtures independently executed','outer witness held only four top-level process handles; helper PID/completion from fixed tests','not native credit or Core/HTTP/SSE','no unexpected API fault injection','control response observed during pending, no <=500ms ACK measurement','no full host-death/process-tree or same-user adversary isolation'],'product_pass_credit':0}
save(RUN/'result.json',result);print(json.dumps({'status':result['status'],'passed':result['substantive_passed'],'inputs':len(expected)}),flush=True)
raise SystemExit(0 if result['status']=='independent_fixed_platform_tests_passed' else 1)
