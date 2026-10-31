"""Generate the independent public upload-history Cap'n Proto Dart contract."""
from pathlib import Path
from urllib.parse import unquote,urljoin,urlparse
import hashlib,json,os,shutil,subprocess,sys,tempfile
from capnp_web_metadata import adapt

ROOT=Path(__file__).resolve().parents[1]
SCHEMA=ROOT/'extensions/blob-upload-history-v1/contracts/blob_upload_history.capnp'
TARGET=ROOT/'lib/plugins/generated'
CONFIG=ROOT/'packages/morrow_core_client/.dart_tool/package_config.json'

def main():
    if sys.argv[1:] not in ([],['--check']):
        raise SystemExit('Usage: generate_blob_history_client.py [--check]')
    schema_sha=hashlib.sha256(SCHEMA.read_bytes().replace(b'\r\n',b'\n')).hexdigest()
    if schema_sha!=(SCHEMA.parent/'schema.sha256').read_text().strip():
        raise SystemExit('Public history schema digest differs from its contract')
    packages=json.loads(CONFIG.read_text(encoding='utf-8'))['packages']
    entry=next(p for p in packages if p['name']=='capnpc_dart')
    package=unquote(urlparse(urljoin(CONFIG.as_uri(),entry['rootUri'])).path)
    if os.name=='nt' and package.startswith('/'):package=package[1:]
    generator=Path(package)/'bin/capnpc_dart.dart'
    dart=Path(os.environ.get('DART_BIN') or shutil.which('dart') or 'dart')
    if dart.suffix.lower()=='.bat':
        candidate=dart.parent/'cache/dart-sdk/bin/dart.exe'
        if candidate.exists():dart=candidate
    capnp=os.environ.get('CAPNP') or shutil.which('capnp') or 'capnp'
    request=subprocess.check_output([str(capnp),'compile','-o-',
        '--src-prefix='+str(SCHEMA.parent),str(SCHEMA)],cwd=ROOT)
    with tempfile.TemporaryDirectory(prefix='history-dart-codegen-') as temporary:
        folder=Path(temporary)
        subprocess.run([str(dart),'--packages='+str(CONFIG),str(generator)],
            input=request,cwd=folder,check=True)
        subprocess.run([str(dart),'format',str(folder)],check=True,stdout=subprocess.DEVNULL)
        name='blob_upload_history.capnp.dart'
        outputs=adapt(name,(folder/name).read_text(encoding='utf-8'),
            generator='tool/generate_blob_history_client.py',exact64=True)
        outputs['blob_upload_history_identity.dart']=(
            '// Public contract digest; independent of private host controls.\n'
            'const historySchemaSha256 = <int>['+
            ','.join(str(n) for n in bytes.fromhex(schema_sha))+'];\n')
        for name,contents in outputs.items():
            (folder/name).write_text(contents,encoding='utf-8')
        subprocess.run([str(dart),'format',str(folder)],check=True,stdout=subprocess.DEVNULL)
        for name in outputs:
            contents=(folder/name).read_text(encoding='utf-8');target=TARGET/name
            if '--check' in sys.argv:
                if not target.is_file() or target.read_text(encoding='utf-8')!=contents:
                    raise SystemExit('Stale public history binding: '+name)
            else:
                previous=target.read_bytes() if target.exists() else b''
                newline='\r\n' if previous and previous.count(b'\r\n')==previous.count(b'\n') else '\n'
                TARGET.mkdir(parents=True,exist_ok=True)
                target.write_bytes(contents.replace('\n',newline).encode())
    print('Verified public history Dart bindings' if '--check' in sys.argv else 'Generated public history Dart bindings')

if __name__=='__main__':main()
