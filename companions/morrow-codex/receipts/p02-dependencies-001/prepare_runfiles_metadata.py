import hashlib, json, pathlib, urllib.request, time
base = pathlib.Path(__file__).resolve().parent
repo = 'hermeticbuild/rules_rust'
sha = 'b56cbaa8465e74127f1ea216f813cd377295ad81'
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
def get(label, tail):
    url = 'https://api.github.com/repos/' + repo + '/git/' + tail
    started = time.time()
    try:
        with opener.open(urllib.request.Request(url, headers={'User-Agent':'morrow-codex-fixed-dependency-preparation'}), timeout=30) as response:
            data = response.read(2000001)
            assert len(data) <= 2000000
            target = base / ('runfiles-' + label + '-direct-001.json')
            target.open('xb').write(data)
            obj = json.loads(data)
            print(json.dumps({'label':label,'sha':obj.get('sha'),'entries':len(obj.get('tree',[])) if isinstance(obj.get('tree'),list) else None,'response_sha256':hashlib.sha256(data).hexdigest()}),flush=True)
            return obj
    except Exception as e:
        (base / ('runfiles-' + label + '-failure-001.json')).open('x').write(json.dumps({'url':url,'error':repr(e),'elapsed':time.time()-started}))
        raise
commit=get('commit','commits/' + sha)
assert commit['sha']==sha
tree=get('root-tree','trees/' + commit['tree']['sha'])
rust=next(e for e in tree['tree'] if e['path']=='rust' and e['type']=='tree')
rust_tree=get('rust-tree','trees/' + rust['sha'])
runfiles=next(e for e in rust_tree['tree'] if e['path']=='runfiles' and e['type']=='tree')
package_tree=get('package-tree','trees/' + runfiles['sha'] + '?recursive=1')
print(json.dumps({'package_entries':package_tree['tree'],'root_license_entries':[e for e in tree['tree'] if 'LICENSE' in e['path'] or 'NOTICE' in e['path']]}),flush=True)
