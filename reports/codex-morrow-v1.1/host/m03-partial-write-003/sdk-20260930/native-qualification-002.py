from pathlib import Path
import os,json,hashlib,subprocess,time
root=Path.cwd();out=root/'reports/codex-morrow-v1.1/host/m03-partial-write-003/native-20260930-002';out.mkdir()
env=os.environ.copy()
vcroot=Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC');v=(vcroot/'Auxiliary/Build/Microsoft.VCToolsVersion.default.txt').read_text().strip();vc=vcroot/'Tools/MSVC'/v
sdk=Path(r'C:\Program Files (x86)\Windows Kits\10');sv=sorted((sdk/'Lib').iterdir(),key=lambda p:p.name)[-1].name
env['PATH']=str(vc/'bin/Hostx64/x64')+os.pathsep+env['PATH']
env['LIB']=';'.join(str(p) for p in [vc/'lib/x64',sdk/'Lib'/sv/'ucrt/x64',sdk/'Lib'/sv/'um/x64'])
env['INCLUDE']=';'.join(str(p) for p in [vc/'include',sdk/'Include'/sv/'ucrt',sdk/'Include'/sv/'um',sdk/'Include'/sv/'shared'])
env['MORROW_HTTP_TEST_ROOT']=str(out/'api-profiles');(out/'api-profiles').mkdir()
env['MORROW_OWNER_TEST_ROOT']=str(out/'owner-profiles');(out/'owner-profiles').mkdir()
env['MORROW_OWNER_TEST_CLIENT']=str(root/'target/m03-stream-001-native/debug/morrow-native-close-peer.exe')
def hashes():
 return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for name in ['native_session_stream_001','network_node_stream_001','native_pipe_win_001'] for p in (root/name).rglob('*') if p.is_file() and 'target' not in p.parts}
r={'sources_before':hashes(),'scope':'native crate library tests and binary compilation; not full product G0','commands':[]}
for label,flags in [('native-build',['build','--bins']),('native-tests',['test','--lib','--','--test-threads=1'])]:
 cmd=[r'C:\Users\Administrator\.cargo\bin\cargo.exe',flags[0],'--locked','--offline','--manifest-path','native_session_stream_001/Cargo.toml','--target-dir','target/m03-stream-001-native',*flags[1:]]
 start=time.monotonic()
 with (out/(label+'.log')).open('wb') as f: result=subprocess.run(cmd,env=env,stdout=f,stderr=subprocess.STDOUT)
 r['commands'].append({'label':label,'command':cmd,'exit_code':result.returncode,'seconds':time.monotonic()-start})
 r['sources_after']=hashes();r['source_unchanged']=r['sources_before']==r['sources_after'];(out/'receipt.json').write_text(json.dumps(r,indent=2))
 print(label,result.returncode,flush=True)
 if result.returncode:break
