"""Independent P02 batch review; every output is beneath this joint directory."""
from pathlib import Path
import hashlib, json, os, re, shutil, subprocess, sys, tomllib, difflib, traceback
sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
PLUGIN = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
RUN = HERE/'runs/p02-native-probe-001-review'
PROBE = PLUGIN/'qualification/p02-native-probe-001'
TOOL = Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
PIN = '1fc5833e632bb3afab827a9ca3519418ab7afdf34c6e50e6cafa59ecce707020'
def sha(b): return hashlib.sha256(b).hexdigest()
def digest(p): return sha(p.read_bytes())
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def require(ok, message):
    if not ok: raise RuntimeError(message)
def pinned():
    p=PLUGIN/'receipts/p02-native-probe-001/handoff.json'
    require(digest(p)==PIN,'new handoff drift')
    h=read(p); paths=dict(h['input_sha256'])
    old=PLUGIN/'receipts/handoff.json'
    require(digest(old)==h['original_frozen_handoff_sha256'],'original handoff drift')
    frozen=read(old)['input_sha256']; require(len(frozen)==17 and len(paths)==35,'input counts')
    paths.update(frozen); paths[h['artifact']['path']]=h['artifact']['sha256']
    paths['receipts/handoff.json']=h['original_frozen_handoff_sha256']
    paths['receipts/p02-native-probe-001/handoff.json']=PIN
    for name, expected in paths.items(): require(digest(PLUGIN/name)==expected,'pin mismatch '+name)
    return paths
def members(root):
    out=set()
    for folder, dirs, files in os.walk(root,followlinks=False):
        for name in list(dirs):
            p=Path(folder)/name
            if p.is_symlink(): out.add(p.relative_to(root).as_posix()); dirs.remove(name)
        out.update((Path(folder)/name).relative_to(root).as_posix() for name in files)
    return out
def verify_files(root, records):
    require(members(root)=={f['path'] for f in records},'file membership '+str(root))
    nodes={}; total=0
    for f in records:
        p=root/f['path']; require(p.resolve().is_relative_to(root.resolve()),'source link escape')
        mode=f.get('mode',f.get('git_mode'))
        require(mode in ('100644','100755','120000'),'unknown source mode')
        require(p.is_symlink()==(mode=='120000'),'link mode mismatch')
        b=os.readlink(p).encode() if p.is_symlink() else p.read_bytes()
        blob=hashlib.sha1(b'blob '+str(len(b)).encode()+b'\0'+b).hexdigest()
        require(len(b)==f['size'] and sha(b)==f['sha256'] and blob==f['git_blob_sha1'],'source bytes '+str(p))
        total+=len(b); node=nodes
        pieces=f['path'].split('/')
        for name in pieces[:-1]: node=node.setdefault(name,{})
        node[pieces[-1]]=(mode,blob)
    trees={}
    def tree(node, prefix=''):
        body=b''
        for name,value in sorted(node.items(),key=lambda kv:(kv[0]+('/' if isinstance(kv[1],dict) else '')).encode()):
            mode, blob=('40000',tree(value,prefix+name+'/')) if isinstance(value,dict) else value
            body+=mode.encode()+b' '+name.encode()+b'\0'+bytes.fromhex(blob)
        h=hashlib.sha1(b'tree '+str(len(body)).encode()+b'\0'+body).hexdigest(); trees[prefix.rstrip('/')]=h
        return h
    return {'files':len(records),'bytes':total,'root_tree':tree(nodes),'trees':trees}
def sources():
    inventory=read(PLUGIN/'receipts/p02-source-batch-001/source-inventory.json'); result=[]
    for source in inventory['sources']:
        for key in ('files_manifest','tree_manifest','commit_api','verification_receipt'):
            item=source[key]; require(digest(PLUGIN/item['path'])==item['sha256'],'source linked hash '+key)
        manifest=read(PLUGIN/source['files_manifest']['path']); tree=read(PLUGIN/source['tree_manifest']['path']); commit=read(PLUGIN/source['commit_api']['path'])
        actual=verify_files(PLUGIN/source['path'],manifest['files'])
        require(actual['root_tree']==source['tree_sha1']==manifest['tree_sha1']==commit['tree']['sha'],'source root tree')
        require(commit['sha']==source['commit']==manifest['commit'],'source commit')
        require(actual['files']==source['file_count'] and actual['bytes']==source['source_bytes'],'source counts')
        expected_trees={e['path']:e['sha'] for e in tree['entries'] if e['type']=='tree'}
        require(actual['trees']==expected_trees and len(expected_trees)==source['tree_count_including_root'],'every git tree identity')
        blobs={e['path']:(e['mode'],e['sha']) for e in tree['entries'] if e['type']=='blob'}
        require(blobs=={f['path']:(f['mode'],f['git_blob_sha1']) for f in manifest['files']},'tree blob members')
        result.append(dict(id=source['id'],commit=source['commit'],files=actual['files'],bytes=actual['bytes'],root_tree=actual['root_tree'],trees=len(expected_trees)))
    return result
def forks():
    results=[]
    for pkg in read(PLUGIN/'receipts/p02-dependency-cache-001/build-forks-001/result.json')['packages']:
        orig,copy=Path(pkg['original']),Path(pkg['copy']); proof=read(Path(pkg['proof_path']))
        require(digest(Path(pkg['proof_path']))==pkg['proof_sha256'],'fork proof hash')
        files=proof['files']
        # runfiles is a package subtree, with receipt paths possibly repository-relative.
        if files and not (orig/files[0]['path']).exists():
            files=[dict(f,path=f['path'].removeprefix('rust/runfiles/')) for f in files]
        checked=verify_files(orig,files)
        if pkg['name']!='runfiles': require(checked['root_tree']==proof['git_tree_sha1'],'fork original tree')
        require(members(copy)==members(orig),'fork membership')
        changed=sorted(p for p in members(orig) if (orig/p).read_bytes()!=(copy/p).read_bytes())
        require(changed==pkg['changed_files'],'undeclared fork patch')
        if changed:
            a=tomllib.loads((orig/'Cargo.toml').read_text()); b=tomllib.loads((copy/'Cargo.toml').read_text())
            dependency=a['dependencies']['tungstenite']; require(dependency.pop('rev')=='4fffad30fe373adbdcffab9545e9e9bf4f2fc19f','fork revision')
            require(dependency.pop('git')=='https://github.com/openai-oss-forks/tungstenite-rs','fork URL'); dependency['path']='../tungstenite'
            require(a==b,'fork patch changes more than dependency source')
        results.append(dict(name=pkg['name'],files=len(files),changed_files=changed,original_root_tree=checked['root_tree']))
    return results
def graph():
    meta=read(PLUGIN/'receipts/p02-native-probe-001/runs/lock-20260928T121520Z-4abd282d33/stdout.txt')
    lock=tomllib.loads((PROBE/'Cargo.lock').read_text())['package']; pkgs=meta['packages']
    reg=[p for p in lock if p.get('source')]; local=[p for p in lock if not p.get('source')]
    require((len(pkgs),len(lock),len(reg),len(local))==(598,598,577,21),'graph counts')
    require({(p['name'],p['version'],p.get('source')) for p in lock}=={(p['name'],p['version'],p.get('source')) for p in pkgs},'metadata lock identities')
    upstream=tomllib.loads((PLUGIN/'upstream/p02-source-batch-001/codex-source/codex-rs/Cargo.lock').read_text())['package']
    kit=tomllib.loads((PLUGIN/'sdk/host-kit-003-copy/Cargo.lock').read_text())['package']
    def identities(items): return {(p['name'],p['version'],p.get('source'),p.get('checksum')) for p in items}
    extra=identities(reg)-identities(upstream)
    require(len(extra)==3 and extra<=identities(kit),'registry provenance drift')
    for p in reg:
        require(p['source']=='registry+https://github.com/rust-lang/crates.io-index','unexpected source')
    return dict(resolved_packages=598,registry_packages=577,local_packages=21,registry_unchanged_from_codex=574,kit_additions=sorted(extra))
def runtime(p):
    value=read(p); require(value['status']=='passed_limited_net_callsite_probe' and len(value['cases'])==2,'runtime status')
    body={'model':'morrow-qualification-only','instructions':'Offline callsite interception fixture. No model request may be sent.','input':[],'tool_choice':'none','parallel_tool_calls':False,'reasoning':None,'store':False,'stream':True,'include':[]}
    encoded=json.dumps(body,separators=(',',':')).encode(); expected=sha(encoded)
    require(len(encoded)==241 and expected=='e8a1ef22ef30a6c5a3ddb41be40e737a25adc091fdc87abf49dba5cea0075950','independent body fixture')
    for n,case in enumerate(value['cases']):
        require(case['name']==['host_disconnected_before_open','unapproved_destination_rejected'][n],'case names')
        assertions=case['assertions']; require(len(assertions)==[8,7][n] and len(dict(assertions))==len(assertions) and all(v is True for _,v in assertions),'assertions')
        e=case['evidence']; require(e['stream_calls']==1 and e['unexpected_execute_calls']==0 and e['commit_attempts']==0 and e['host_exchange_calls']==1-n and e['open_attempts']==1-n,'call counts')
        require(len(e['requests'])==1 and len(e['host_frames'])==1-n,'records counts'); r=e['requests'][0]
        require(r['method']=='POST' and r['url']==['https://morrow-offline-fixture.invalid/v1/responses','https://unapproved-fixture.invalid/v1/responses'][n],'request destination')
        require(r['prepared_body_json']==body and r['prepared_body_sha256']==expected and r['prepared_body_length']==241,'request body')
        require(dict(r['headers'])=={'accept':'text/event-stream','content-type':'application/json'},'headers')
        require(['MORROW_P02_HOST_DISCONNECTED_BEFORE_OPEN','MORROW_P02_DESTINATION_OR_METHOD_DENIED'][n] in case['upstream_returned_error'],'sentinel')
        if n==0:
            f=e['host_frames'][0]
            require((f['kind'],f['request_id'],f['session_id'],f['instance_epoch'],f['attempt_id'],f['request_body_length'],f['request_body_sha256'])==('Stream.Open',1,'p02-disconnected-session',1,'p02-disconnected-attempt',241,expected),'frame binding')
    return value
def build():
    consumer=RUN/'consumer'; crate=consumer/'qualification/p02-native-probe-001'
    for p in (crate/'src',consumer/'sdk/host-kit-003-copy',consumer/'receipts/p02-native-probe-001',RUN/'cargo-home',RUN/'tmp',RUN/'profile/AppData/Local',RUN/'profile/AppData/Roaming'): p.mkdir(parents=True,exist_ok=True)
    original=(PROBE/'Cargo.toml').read_text()
    relocated=re.sub(r'path\s*=\s*"([^"]+)"',lambda m:'path = "'+(PROBE/m[1]).resolve().as_posix()+'"',original)
    # The bin source is also a path: keep it local and byte-identical.
    relocated=relocated.replace((PROBE/'src/main.rs').as_posix(),'src/main.rs')
    (crate/'Cargo.toml').write_text(relocated,encoding='utf-8')
    (RUN/'consumer-manifest.patch').write_text(''.join(difflib.unified_diff(original.splitlines(True),relocated.splitlines(True),fromfile='producer/Cargo.toml',tofile='consumer/Cargo.toml')),encoding='utf-8')
    for rel in ('Cargo.lock','src/main.rs'): shutil.copyfile(PROBE/rel,crate/rel)
    shutil.copyfile(PLUGIN/'sdk/host-kit-003-copy/manifest.json',consumer/'sdk/host-kit-003-copy/manifest.json')
    shutil.copyfile(PLUGIN/'out/p02-native-probe-001/cargo-home/config.toml',RUN/'cargo-home/config.toml')
    allowed={'SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PATH','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR','WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION','VSCMD_ARG_TGT_ARCH','VSCMD_ARG_HOST_ARCH'}
    env={k:v for k,v in os.environ.items() if k.upper() in allowed}
    env.update(CARGO_HOME=str(RUN/'cargo-home'),CARGO_TARGET_DIR=str(RUN/'target'),CARGO_NET_OFFLINE='true',CARGO_TERM_COLOR='never',CARGO_INCREMENTAL='0',RUSTC=str(TOOL/'rustc.exe'),RUSTDOC=str(TOOL/'rustdoc.exe'),RUSTUP_AUTO_INSTALL='0',HOME=str(RUN/'profile'),USERPROFILE=str(RUN/'profile'),LOCALAPPDATA=str(RUN/'profile/AppData/Local'),APPDATA=str(RUN/'profile/AppData/Roaming'),TEMP=str(RUN/'tmp'),TMP=str(RUN/'tmp'),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_SYSTEM=os.devnull,GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0',GIT_NO_LAZY_FETCH='1')
    env['PATH']=str(TOOL)+os.pathsep+r'C:\Users\Administrator\capnp-bin'+os.pathsep+env.get('PATH','')
    for p in (Path('C:/.cargo/config'),Path('C:/.cargo/config.toml')): require(not p.exists(),'root config')
    commands=[]
    def command(label,args,timeout=1200):
        with (RUN/(label+'.stdout')).open('wb') as out,(RUN/(label+'.stderr')).open('wb') as err:
            result=subprocess.run(args,cwd='C:/',env=env,stdout=out,stderr=err,timeout=timeout)
        commands.append(dict(stage=label,argv=args,exit_code=result.returncode)); write(RUN/'commands.json',commands)
        require(result.returncode==0,label+' failed; inspect joint logs')
    command('rustc',[str(TOOL/'rustc.exe'),'-vV'],30)
    command('cargo',[str(TOOL/'cargo.exe'),'--version'],30)
    command('metadata',[str(TOOL/'cargo.exe'),'metadata','--locked','--offline','--format-version','1','--manifest-path',str(crate/'Cargo.toml')])
    old=read(PLUGIN/'receipts/p02-native-probe-001/runs/lock-20260928T121520Z-4abd282d33/stdout.txt'); new=read(RUN/'metadata.stdout')
    oldroot=old['resolve']['root']; newroot=new['resolve']['root']
    def normalize(value):
        if isinstance(value,str): return value.replace(newroot,oldroot).replace(str(crate).replace('\\','/'),str(PROBE).replace('\\','/')).replace(str(crate),str(PROBE)).replace(consumer.as_uri(),PLUGIN.as_uri())
        if isinstance(value,list): return [normalize(v) for v in value]
        if isinstance(value,dict): return {k:normalize(v) for k,v in value.items()}
        return value
    require(normalize(new['resolve'])==old['resolve'],'resolved graph semantic drift')
    def package_identity(packages): return sorted((p['name'],p['version'],p['source'],normalize(p['manifest_path'])) for p in packages)
    require(package_identity(new['packages'])==package_identity(old['packages']),'resolved package identity drift')
    write(RUN/'consumer-relocation.json',dict(original_manifest_sha256=digest(PROBE/'Cargo.toml'),consumer_manifest_sha256=digest(crate/'Cargo.toml'),source_bytes_equal=(PROBE/'src/main.rs').read_bytes()==(crate/'src/main.rs').read_bytes(),lock_bytes_equal=(PROBE/'Cargo.lock').read_bytes()==(crate/'Cargo.lock').read_bytes(),resolved_graph_equal_except_consumer_root_location=True))
    print('Static identity and consumer resolved graph passed; starting isolated offline build',flush=True)
    command('build',[str(TOOL/'cargo.exe'),'build','--locked','--offline','--message-format=json-render-diagnostics','--manifest-path',str(crate/'Cargo.toml'),'--target','x86_64-pc-windows-msvc','--bin','p02-native-probe'])
    exe=RUN/'target/x86_64-pc-windows-msvc/debug/p02-native-probe.exe'; receipt=consumer/'receipts/p02-native-probe-001/joint-runtime.json'
    command('runtime',[str(exe),str(receipt)],30)
    actual=runtime(receipt); producer=runtime(PLUGIN/read(PLUGIN/'receipts/p02-native-probe-001/handoff.json')['runtime_receipt'])
    require(actual==producer,'independent runtime differs')
    require((PROBE/'Cargo.lock').read_bytes()==(crate/'Cargo.lock').read_bytes(),'consumer lock changed')
    return dict(executable=str(exe),exe_sha256=digest(exe),producer_exe_sha256=read(PLUGIN/'receipts/p02-native-probe-001/handoff.json')['artifact']['sha256'],runtime_receipt=str(receipt),runtime_sha256=digest(receipt),runtime_equal_to_producer=True,cases=2,assertions=15,commands=commands)
def main():
    RUN.mkdir(parents=True,exist_ok=False); shutil.copyfile(__file__,RUN/'reviewer-source.py')
    result=dict(status='failed',scope='complete fixed source identities and limited Windows refusal probe',P02='blocked',G0='blocked',product_graphs_built=0,product_acceptance_verified=0,product_acceptance_not_run=84)
    try:
        before=pinned(); write(RUN/'inputs-before.json',before)
        result['sources']=sources(); result['forks']=forks(); result['graph']=graph()
        runtime(PLUGIN/read(PLUGIN/'receipts/p02-native-probe-001/handoff.json')['runtime_receipt'])
        write(RUN/'static-review.json',result)
        result['independent_consumer']=build()
        after=pinned(); require(after==before,'frozen input drift'); write(RUN/'inputs-after.json',after)
        require(sources()==result['sources'] and forks()==result['forks'],'source/fork drift after build')
        result['original_inputs_unchanged']=True; result['status']='verified_limited'; result['source_download_block_resolved']=True
    except Exception as exc:
        result['error']=str(exc); (RUN/'exception.txt').write_text(traceback.format_exc(),encoding='utf-8')
    write(RUN/'result.json',result); print(json.dumps(result,ensure_ascii=False)); return 0 if result['status']=='verified_limited' else 1
if __name__=='__main__': raise SystemExit(main())
