"""Read-only decoder of the unchanged host Capnp001 authority, not a new wire."""
import ast,re,subprocess
from pathlib import Path
from check_review import HOST,sha
SCHEMA=HOST/'contracts/experimental/agent_host_v2_capnp/native_session.capnp'
CAPNP=Path('C:/Users/Administrator/capnp-bin/capnp.exe')
def decode(raw):
    if sha(SCHEMA)!='fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450' or sha(CAPNP)!='c41ed4ec9c9d8ed6f3334fd9a8186d7196d890db2bd540bb15ed5cec8b49ef34':raise ValueError('decoder authority changed')
    if len(raw)<4:raise ValueError('short prefix')
    n=int.from_bytes(raw[:4],'little');payload=raw[4:]
    if not 8<=n<=2048 or n%8 or len(payload)!=n:raise ValueError('outer frame length')
    count=int.from_bytes(payload[:4],'little')+1
    if count>n//4-1:raise ValueError('segment count')
    sizes=[int.from_bytes(payload[4+i*4:8+i*4],'little')for i in range(count)]
    if ((count+2)//2)*8+sum(sizes)*8!=n:raise ValueError('segment/trailing bytes')
    p=subprocess.run([str(CAPNP),'decode',str(SCHEMA),'NativeSession'],input=payload,capture_output=True,timeout=5,creationflags=subprocess.CREATE_NO_WINDOW)
    if p.returncode:raise ValueError('Capnp rejected')
    fields={}
    for name,token in re.findall(r'(\w+)\s*=\s*("(?:[^"\\]|\\.)*"|[A-Za-z][A-Za-z0-9]*|[0-9]+)',p.stdout.decode()):
        if name in fields:raise ValueError('duplicate field')
        fields[name]=ast.literal_eval(token).encode('latin1').hex() if token.startswith('"') else int(token) if token.isdecimal() else token
    if set(fields)!=set(re.findall(r'^\s*(\w+)\s+@\d+\s*:',SCHEMA.read_text(),re.M)):raise ValueError('decoded field set')
    if fields['major']!=2 or fields['revision']!=1 or fields['reserved']!=0:raise ValueError('version/reserved')
    if any(len(fields[k])!=64 for k in ['nonce','schemaSha256','artifactSha256','executionConfigSha256']):raise ValueError('data length')
    return fields
