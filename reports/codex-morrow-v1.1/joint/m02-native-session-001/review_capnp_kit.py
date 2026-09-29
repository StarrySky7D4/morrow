"""Independent capnp CLI consumer of the pinned host-owned v2 Capnp kit."""
import ast
import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path
sys.dont_write_bytecode=True
from check_review import HERE, HOST, sha, read, write, verify_pins

KIT=HOST/'reports/codex-morrow-v1.1/host/m02-native-session-001/capnp-kit-001'
MANIFEST_SHA='2a0146a8f18b658b906c6a5f2d0334272a794edf43b0b6ac5757d9ba45cb66d9'
SCHEMA_SHA='fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450'
SCHEMA=HOST/'contracts/experimental/agent_host_v2_capnp/native_session.capnp'
CAPNP=Path('C:/Users/Administrator/capnp-bin/capnp.exe')

def authority():
    if sha(KIT/'manifest.json')!=MANIFEST_SHA: raise ValueError('kit manifest changed')
    manifest=read(KIT/'manifest.json')
    if Path(manifest['authority']).resolve()!=SCHEMA.parent or sha(SCHEMA)!=SCHEMA_SHA: raise ValueError('authority changed')
    pins=[]
    for root,key in [(KIT,'files'),(SCHEMA.parent,'authority_files')]:
        for relative,digest in manifest[key].items():
            path=(root/relative).resolve()
            if not path.is_relative_to(root) or '..' in Path(relative).parts: raise ValueError('unsafe manifest path')
            pins.append({'path':str(path),'sha256':digest})
    actual={p.relative_to(KIT).as_posix() for p in KIT.rglob('*') if p.is_file()}
    if actual!=set(manifest['files'])|{'manifest.json'}: raise ValueError('kit file set changed')
    checked,issues=verify_pins(pins)
    if issues: raise ValueError(issues)
    return checked

def payload(frame):
    if len(frame)<4: raise ValueError('short prefix')
    n=int.from_bytes(frame[:4],'little')
    if not 8<=n<=2048 or n%8 or len(frame)!=n+4: raise ValueError('outer length/alignment')
    data=frame[4:]
    count=int.from_bytes(data[:4],'little')+1
    if count>n//4-1: raise ValueError('segment table count')
    table_bytes=((count+2)//2)*8
    sizes=[int.from_bytes(data[4+i*4:8+i*4],'little') for i in range(count)]
    if table_bytes+sum(sizes)*8!=n: raise ValueError('segments/trailing bytes')
    return data

def parse_text(text):
    fields={}
    for name,token in re.findall(r'(\w+)\s*=\s*("(?:[^"\\]|\\.)*"|[A-Za-z][A-Za-z0-9]*|[0-9]+)',text):
        if name in fields: raise ValueError('multiple messages or duplicate fields')
        if token.startswith('"'): value=ast.literal_eval(token).encode('latin1').hex()
        elif token.isdecimal(): value=int(token)
        else: value=token
        fields[name]=value
    expected=set(re.findall(r'^\s*(\w+)\s+@\d+\s*:',SCHEMA.read_text(encoding='utf-8'),re.M))
    if set(fields)!=expected: raise ValueError('decoded field set mismatch')
    for key in ['nonce','schemaSha256','artifactSha256','executionConfigSha256']:
        if len(fields[key])!=64: raise ValueError('digest length:'+key)
    if fields['major']!=2 or fields['revision']!=1 or fields['reserved']!=0: raise ValueError('version/reserved')
    if fields['kind'] not in ['challenge','hello','welcome','query','state','denied','stop','close']: raise ValueError('unknown kind')
    return fields

def decode_frame(frame):
    raw=payload(frame)
    proc=subprocess.run([str(CAPNP),'decode',str(SCHEMA),'NativeSession'],input=raw,capture_output=True,timeout=10)
    if proc.returncode: raise ValueError('independent capnp decoder rejected')
    return parse_text(proc.stdout.decode('utf-8')),proc.stdout,proc.stderr

def main():
    import argparse
    ap=argparse.ArgumentParser();ap.add_argument('--run-id',required=True);args=ap.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,90}',args.run_id): ap.error('unsafe run id')
    run=HERE/'runs'/args.run_id;run.parent.mkdir(exist_ok=True);run.mkdir(exist_ok=False)
    result={'scope':'new_capnp_kit_independent_decoding_only','runtime_accepted':False,
      'product_pass_credit':0,'reviewer_sha256':sha(__file__),'manifest_sha256':MANIFEST_SHA,
      'schema_sha256':SCHEMA_SHA,'capnp_cli_sha256':sha(CAPNP),'checks':[]}
    try:
        pins=authority();write(run/'identities-before.json',pins)
        expected={'challenge':('challenge',0,0),'hello':('hello',1,0),'welcome':('welcome',1,2),
          'query':('query',2,0),'state':('state',2,2),'denied-revoked':('denied',3,19),
          'stop':('stop',0,25),'close':('close',3,0)}
        for name,(kind,seq,code) in expected.items():
            frame=(KIT/'vectors'/f'{name}.frame').read_bytes()
            if payload(frame)!=(KIT/'vectors'/f'{name}.capnp').read_bytes(): raise ValueError('frame/payload mismatch')
            f,out,err=decode_frame(frame)
            (run/f'{name}.decoded.txt').write_bytes(out);(run/f'{name}.stderr.txt').write_bytes(err)
            wanted={'major':2,'revision':1,'kind':kind,'sequence':seq,'session':1,'instanceEpoch':2,
              'revocationGeneration':2 if kind=='denied' else 1,'childPid':42,'code':code,
              'remainingMs':1000,'nonce':'07'*32,'schemaSha256':SCHEMA_SHA,
              'artifactSha256':'08'*32,'executionConfigSha256':'09'*32,
              'requestBudget':16,'capabilities':1,'reserved':0}
            if f!=wanted: raise ValueError('semantic vector mismatch:'+name)
            result['checks'].append({'name':name,'status':'verified','frame_bytes':len(frame),
              'frame_sha256':hashlib.sha256(frame).hexdigest(),'decoded':f})
        for name in ['oversize-prefix','truncated','bad-root']:
            frame=(KIT/'vectors'/f'{name}.invalid').read_bytes()
            try: decode_frame(frame)
            except ValueError as exc: result['checks'].append({'name':name,'status':'rejected_as_expected','reason':str(exc)})
            else: raise ValueError('invalid accepted:'+name)
        pins_after=authority();write(run/'identities-after.json',pins_after)
        if pins_after!=pins: raise ValueError('kit identity changed during review')
        result.update(status='verified_limited',exit_code=0,positive_vectors=8,negative_vectors=3,
          identity_count=len(pins),independent_rust_build=False,
          limits='CLI decode and explicit frame/field validation only; host Rust traversal/nesting failure behavior still requires targeted consumer test')
    except Exception as exc:
        result.update(status='failed',exit_code=1,error=type(exc).__name__,message=str(exc))
    write(run/'result.json',result);print(json.dumps(result,ensure_ascii=False,indent=2));return result['exit_code']

if __name__=='__main__': raise SystemExit(main())
