import hashlib, json, pathlib, urllib.request, time
receipt_dir = pathlib.Path(__file__).resolve().parent
project = receipt_dir.parent.parent
sha = 'b56cbaa8465e74127f1ea216f813cd377295ad81'
repo = 'hermeticbuild/rules_rust'
output = project / 'upstream' / 'p02-dependencies-001' / ('rules_rust-' + sha + '-runfiles-package')
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
def git_hash(kind, data):
    return hashlib.sha1(kind.encode()+b' '+str(len(data)).encode()+b'\0'+data).hexdigest()
def load(label):
    p=receipt_dir / ('runfiles-' + label + '-direct-001.json')
    return json.loads(p.read_bytes())
def tree_hash(entries):
    entries=sorted(entries,key=lambda e:(e['path']+('/' if e['type']=='tree' else '')).encode())
    data=b''.join(e['mode'].lstrip('0').encode()+b' '+e['path'].encode()+b'\0'+bytes.fromhex(e['sha']) for e in entries)
    return git_hash('tree',data)
commit=load('commit'); root=load('root-tree'); rust=load('rust-tree'); package=load('package-tree')
assert commit['sha']==sha
assert root.get('truncated') is False and rust.get('truncated') is False and package.get('truncated') is False
assert tree_hash(root['tree'])==root['sha']==commit['tree']['sha']
assert tree_hash(rust['tree'])==rust['sha']==next(e['sha'] for e in root['tree'] if e['path']=='rust')
children=[e for e in package['tree'] if '/' not in e['path']]
assert tree_hash(children)==package['sha']==next(e['sha'] for e in rust['tree'] if e['path']=='runfiles')
data_children=[dict(e,path=e['path'].split('/',1)[1]) for e in package['tree'] if e['path'].startswith('data/')]
assert tree_hash(data_children)==next(e['sha'] for e in children if e['path']=='data')
entries=[dict(e,path='rust/runfiles/'+e['path']) for e in package['tree'] if e['type']=='blob']
entries += [e for e in root['tree'] if e['path']=='LICENSE.txt']
assert len(entries)==6
output.mkdir(exist_ok=False)
manifest=[]
for entry in entries:
    path=entry['path']; assert entry['mode']=='100644' and '..' not in pathlib.PurePosixPath(path).parts
    url='https://raw.githubusercontent.com/'+repo+'/'+sha+'/'+path
    attempt={'url':url,'path':path,'expected_blob':entry['sha']}
    try:
        with opener.open(urllib.request.Request(url,headers={'User-Agent':'morrow-codex-fixed-dependency-preparation'}),timeout=30) as response:
            data=response.read(1000000)
        assert len(data)==entry['size'] and git_hash('blob',data)==entry['sha']
        target=output / pathlib.PurePosixPath(path); target.parent.mkdir(parents=True,exist_ok=True)
        target.open('xb').write(data)
        attempt.update(status='verified',size=len(data),git_mode=entry['mode'],git_blob_sha1=git_hash('blob',data),sha256=hashlib.sha256(data).hexdigest())
        manifest.append(attempt)
        print(path+': verified',flush=True)
    except Exception as e:
        attempt.update(status='failed_preserved',error=repr(e))
        raise
    finally:
        (receipt_dir / ('runfiles-file-'+str(len(manifest))+'-attempt-001.json')).open('x').write(json.dumps(attempt,indent=2)+'\n')
result={'status':'complete_package_subset_verified','original_repository':'dzbarsky/rules_rust','redirected_repository':repo,'commit':sha,'root_tree_sha1':root['sha'],'rust_tree_sha1':rust['sha'],'package_tree_sha1':package['sha'],'source_root':str(output),'cargo_manifest':str(output/'rust/runfiles/Cargo.toml'),'files':manifest,'scope':'Complete rust/runfiles directory plus repository LICENSE.txt; not complete rules_rust repository','validation':'Root, rust, runfiles and nested data Git tree hashes reconstructed and bound to fixed commit; each selected raw file matches exact Git blob SHA1 and has SHA256','limits':['No compiler/Cargo/runtime execution','Source references must still be reviewed for package-external includes','No existing lock/probe/upstream originals changed']}
(receipt_dir / 'runfiles-package-verification.json').open('x').write(json.dumps(result,indent=2)+'\n')
print('runfiles package subset complete',flush=True)
