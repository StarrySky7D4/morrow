"""Construct wire only; actual native EventEnvelope is the verdict/encoder oracle.

SEVC0001 + u32 count; ten u32 fields per record: name length, accept, encode
success, has retry, retry low/high, data/event/id lengths, wire length; then name,
expected data/event/id UTF8 bytes, wire. UTF8 strings have explicit byte lengths.
"""
import hashlib,json,struct
from pathlib import Path
W=lambda v:struct.pack('<Q',v)
U=lambda v:struct.pack('<I',v)
D=bytes.fromhex('bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863')
def pad(b):return b+b'\0'*((-len(b))%8)
def sp(off=0,ds=2,ps=4):return ((off&0x3fffffff)<<2)|(ds<<32)|(ps<<48)
def lp(off,n):return 1|((off&0x3fffffff)<<2)|(2<<32)|(n<<35)
def far(seg,off=0,double=False):return 2|(int(double)<<2)|(off<<3)|(seg<<32)
def frame(segs):
 h=U(len(segs)-1)+b''.join(U(len(s)//8) for s in segs)
 return h+b'\0'*((-len(h))%8)+b''.join(segs)
def envelope(d=b'',e=b'',i=b'',retry=None,ds=2,ps=4,digest=D,version=1,nulls=()):
 fields=[digest,d+b'\0',e+b'\0',i+b'\0'];idx=1+ds+ps;targets=[];chunks=[]
 for n,b in enumerate(fields):
  targets.append(idx);chunk=pad(b);idx+=len(chunk)//8;chunks.append(chunk)
 words=[sp(ds=ds,ps=ps),version|(int(retry is not None)<<16)]
 if ds>1:words+=[retry or 0]
 words+=[0]*max(0,ds-2)
 words+=[0 if n in nulls else lp(targets[n]-(1+ds+n)-1,len(fields[n])) for n in range(min(4,ps))]
 words+=[0]*max(0,ps-4)
 return frame([b''.join(map(W,words))+b''.join(chunks)])
rows=[]
def add(name,w,ok=False,d=b'',e=b'',i=b'',retry=None,enc=True,**notes):
 rows.append(dict(name=name,accept=ok,encode_success=bool(ok and enc),has_retry=int(retry is not None),retry=retry or 0,data=d,event=e,id=i,wire=w,**notes))
def good(name,d=b'',e=b'',i=b'',retry=None,**kw):add(name,envelope(d,e,i,retry,**kw),True,d,e,i,retry)
for r,n in [(None,'none'),(0,'zero'),((1<<64)-1,'max')]:good('retry-'+n,retry=r)
good('unicode-multiline-nul-data','雪🙂\nsecond\0line'.encode(),'消息'.encode(),b'i'*300,7)
good('empty-id-reset',b'd',b'message',b'',0)
for field in range(3):
 vals=[b'',b'',b''];vals[field]='内\0嵌🙂'.encode();good('embedded-nul-field-%d'%field,*vals,retry=7)
good('literal-DONE-not-completion',b'[DONE]',b'',b'provider-id')
good('id-over-parser-default-1024',b'd',b'e',b'i'*2048)
good('null-all-texts',nulls=(1,2,3))
good('missing-all-text-fields',ps=1)
good('missing-retry-word-present-zero',retry=0,ds=1)
good('expanded-compatible-struct',b'd',b'e',b'i',7,ds=4,ps=6)
good('large-data-64000',b'x'*64000)
maxwire=envelope(b'x'*65416);assert len(maxwire)==65536
add('decode-max-wire-native-encode-limit',maxwire,True,b'x'*65416,enc=False)
base=envelope();add('empty-input',b'');add('missing-root',frame([b'']));add('null-root',frame([W(0)]));add('missing-data-version',frame([W(sp(ds=0,ps=0))]))
for version in (0,2,65535):add('version-%d'%version,envelope(version=version))
for digest,n in [(b'','missing'),(bytes(32),'wrong'),(D[:31],'short'),(D+b'\0','long')]:add('digest-'+n,envelope(digest=digest))
bad=bytearray(base);bad[8+16]=1;add('has-retry-false-nonzero-retry',bytes(bad))
for p,n in [(b'\xff','ff'),(b'\xc0\xaf','overlong'),(b'\xed\xa0\x80','surrogate'),(b'\xf4\x90\x80\x80','above-unicode'),(b'\xe2\x82','partial')]:
 for field in range(3):
  vals=[b'',b'',b''];vals[field]=p;add('utf8-field-%d-%s'%(field,n),envelope(*vals))
for field in range(3):
 # Native canonical small empty Text is exactly one word, final terminator byte0.
 w=bytearray(base);w[8+(11+field)*8]=ord('x');add('bad-text-terminator-%d'%field,bytes(w))
 w=bytearray(base);struct.pack_into('<Q',w,8+(4+field)*8,lp(0,0));add('nonnull-zero-length-text-%d'%field,bytes(w))
 w=bytearray(base);struct.pack_into('<Q',w,8+(4+field)*8,sp());add('text-is-struct-%d'%field,bytes(w))
 w=bytearray(base);struct.pack_into('<Q',w,8+(4+field)*8,1|(6<<2)|(5<<32)|(1<<35));add('text-word-list-%d'%field,bytes(w))
add('trailing-zero-word',base+bytes(8));add('trailing-byte',base+b'x')
add('wire-over-limit',base+bytes(65537-len(base)))
add('field-over-limit',envelope(b'x'*65536))
for n in (1,4,7,8,9,len(base)-1):add('truncated-%d'%n,base[:n])
add('segment-out-of-bounds',U(0)+U(9999)+W(0))
add('far-unknown-segment',frame([W(far(9))]));add('far-out-of-bounds',frame([W(far(1,99)),W(0)]))
add('double-far-short-landing',frame([W(far(1,double=True)),W(0)]))
add('far-landing-is-far',frame([W(far(1)),W(far(0))]));add('513-empty-segments',frame([b'']*513))
bad=bytearray(base);struct.pack_into('<Q',bad,8+3*8,3);add('digest-capability',bytes(bad))
add('unknown-reachable-capability',frame([W(sp(ps=5))+W(1)+W(0)+W(lp(4,32))+W(0)*3+W(3)+D]))
add('cyclic-unknown-struct',frame([W(sp(ps=5))+W(1)+W(0)+W(lp(4,32))+W(0)*3+W(sp(off=-1,ds=0,ps=1))+D]))
add('nested-unknown-struct-limit',frame([W(sp(ps=5))+W(1)+W(0)+W(lp(4,32))+W(0)*3+W(sp(off=4,ds=0,ps=1))+D+W(sp(ds=0,ps=1))*9+W(0)]))
# One stored Text, referenced by all3fields. Header/wire remain below64KiB.
def aliases(n):
 text=b'x'*n+b'\0'
 words=[sp(),1,0,lp(3,32),lp(6,n+1),lp(5,n+1),lp(4,n+1)]
 return frame([b''.join(map(W,words))+D+pad(text)])
shared=aliases(24576);assert len(shared)<65536
add('shared-24k-text-aggregate-over-limit',shared,wire_semantic_aggregate_hint=73728,wire_below_cap=True)
add('valid-shared-text-aliases',aliases(4096),True,b'x'*4096,b'x'*4096,b'x'*4096)
# Large compatible struct with repeated unknown Data aliases exceeds traversal.
ps=24;di=3+ps;alias=di+4
words=[sp(ps=ps),1,0,lp(di-4,32),0,0,0]+[lp(alias-j-1,8192) for j in range(7,3+ps)]
add('aliased-unknown-traversal-budget',frame([b''.join(map(W,words))+D+bytes(8192)]))
# Real root+independent Data/Text far pointers across6segments.
d='多段\0 data'.encode();e=b'delta';i=b'i'*300
segs=[W(far(1)),W(sp())+W(1|(1<<16))+W(7)+b''.join(W(far(n)) for n in range(2,6)),W(lp(0,32))+D]
segs += [W(lp(0,len(v)+1))+pad(v+b'\0') for v in (d,e,i)]
add('valid-six-segment-single-far',frame(segs),True,d,e,i,7)
ordinary=envelope(d,e,i,(1<<64)-1);body=ordinary[16:]
add('valid-double-far-root',frame([W(far(1,double=True)),W(far(2))+W(sp()),body]),True,d,e,i,(1<<64)-1)
add('valid-unused-empty-segment',frame([base[8:],b'']),True)
# Text objects precede a backwards-pointing compatible root struct.
chunks=[D,pad(b'back\0'),pad(b'event\0'),pad(b'id\0')];targets=[1,5,6,7];rootidx=8
words=[sp(off=rootidx-1)];blob=b''.join(map(W,words))+b''.join(chunks)
blob+=W(1)+W(0)+b''.join(W(lp(t-(rootidx+2+n)-1,len(v))) for n,(t,v) in enumerate(zip(targets,[D,b'back\0',b'event\0',b'id\0'])))
add('valid-backward-text-pointers',frame([blob]),True,b'back',b'event',b'id')
add('valid-unknown-data-alias',frame([W(sp(ps=5))+W(1)+W(0)+W(lp(4,32))+W(0)*3+W(lp(0,32))+D]),True)
root=Path(__file__).resolve().parent;binary=b'SEVC0001'+U(len(rows));inventory=[]
for r in rows:
 name=r['name'].encode();d,e,i,w=[r[k] for k in ('data','event','id','wire')];retry=r['retry']
 binary+=struct.pack('<10I',len(name),int(r['accept']),int(r['encode_success']),r['has_retry'],retry&0xffffffff,retry>>32,len(d),len(e),len(i),len(w))+name+d+e+i+w
 inventory.append({k:v for k,v in r.items() if k not in ('data','event','id','wire')}|{'data_hex':d.hex(),'event_hex':e.hex(),'id_hex':i.hex(),'wire_bytes':len(w),'wire_sha256':hashlib.sha256(w).hexdigest()})
(root/'vectors.bin').write_bytes(binary);(root/'vectors.json').write_text(json.dumps({'format':'SEVC0001','native_oracle_required':True,'vectors':inventory},indent=2)+'\n',encoding='utf8')
print(json.dumps({'vectors':len(rows),'accept':sum(r['accept'] for r in rows),'reject':sum(not r['accept'] for r in rows),'sha256':hashlib.sha256(binary).hexdigest()}))
