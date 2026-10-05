"""Deterministic Capnp wire construction, never a decoder. Native oracle verifies verdicts.

Run in the tests directory. Binary corpus is WSVC0002 + count(u32le), then
eight u32le fields (name length, accept, encode success, kind, has code, code,
payload length, wire length), followed by UTF8 name, expected payload, wire.
"""
import hashlib, json, struct
from pathlib import Path
W=lambda v:struct.pack('<Q',v)
U=lambda v:struct.pack('<I',v)
D=bytes.fromhex('2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d')
def data(p):return p+b'\0'*((-len(p))%8)
def sp(off=0,ds=1,ps=2):return ((off&0x3fffffff)<<2)|(ds<<32)|(ps<<48)
def lp(off,n):return 1|((off&0x3fffffff)<<2)|(2<<32)|(n<<35)
def far(seg,off=0,double=False):return 2|(int(double)<<2)|(off<<3)|(seg<<32)
def frame(segs):
 h=U(len(segs)-1)+b''.join(U(len(s)//8) for s in segs)
 return h+b'\0'*((-len(h))%8)+b''.join(segs)
def envelope(kind=0,p=b'',code=None,ds=1,ps=2,digest=D,version=1,null=False):
 words=[sp(ds=ds,ps=ps),version|(kind<<16)|(int(code is not None)<<32)|((code or 0)<<48)]
 words += [0]*(ds-1)
 dig_idx=1+ds+ps; pay_idx=dig_idx+(len(digest)+7)//8
 words += [lp(dig_idx-(1+ds)-1,len(digest))] if ps else []
 words += [0 if null else lp(pay_idx-(2+ds)-1,len(p))] if ps>1 else []
 words += [0]*max(0,ps-2)
 return frame([b''.join(map(W,words))+data(digest)+data(p)])
cases=[]
def add(name,b,ok=False,k=0,p=b'',c=None,encode=True):
 cases.append(dict(name=name,accept=ok,encode_success=bool(ok and encode),kind=k,has_close_code=int(c is not None),close_code=c or 0,payload=p,wire=b))
def good(name,k,p=b'',c=None,**kw):add(name,envelope(k,p,c,**kw),True,k,p,c)
for k in range(5):good('kind-%d-empty'%k,k)
good('unicode-text',0,'雪🙂\x00 café'.encode())
good('binary-all-byte-values',1,bytes(range(256)))
good('ping-byte-null',2,b'\xff\x00\x80')
good('pong-unicode',3,'é'.encode())
good('null-payload',1,null=True)
good('missing-payload-field',0,ps=1)
good('larger-compatible-data-and-pointers',1,b'compatible',ds=3,ps=5)
good('ping-limit-125',2,b'x'*125)
good('pong-limit-125',3,b'x'*125)
good('close-reason-limit-123',4,b'x'*123,1000)
good('large-binary-64000',1,bytes(range(256))*250)
good('large-binary-65400',1,b'x'*65400)
add('decode-max-envelope-encode-limit',envelope(1,b'x'*65464),True,1,b'x'*65464,encode=False)
for k in (2,3):
 for n in (126,127):add('control-%d-%d'%(k,n),envelope(k,b'x'*n))
for n in (124,125,127):add('close-reason-%d'%n,envelope(4,b'x'*n,1000))
add('close-reason-without-code',envelope(4,b'reason'))
for k in range(4):add('nonclose-code-%d'%k,envelope(k,b'',1000))
for p,n in [(b'\xff','leading-ff'),(b'\xc0\xaf','overlong'),(b'\xed\xa0\x80','surrogate'),(b'\xf4\x90\x80\x80','above-unicode'),(b'\xe2\x82','truncated')]:
 for k in (0,4):add('utf8-%d-%s'%(k,n),envelope(k,p,1000 if k==4 else None))
for code in (0,1,999,1000,1001,1002,1003,1004,1005,1006,1007,1008,1009,1010,1011,1012,1013,1014,1015,1016,2999,3000,3999,4000,4999,5000,65535):
 ok=code in range(1000,1004) or code in range(1007,1014) or code in range(3000,5000)
 add('close-code-%d'%code,envelope(4,b'',code),ok,4,b'',code)
b=envelope();add('empty-input',b'');add('missing-root',frame([b'']));add('null-root',frame([W(0)]))
add('missing-version-data',frame([W(sp(ds=0,ps=0))]))
add('version-zero',envelope(version=0));add('version-two',envelope(version=2))
add('missing-digest',envelope(digest=b''));add('wrong-digest',envelope(digest=bytes(32)))
add('short-digest',envelope(digest=D[:31]));add('long-digest',envelope(digest=D+b'\0'))
add('unknown-kind-five',envelope(kind=5));add('unknown-kind-65535',envelope(kind=65535))
bad=bytearray(b);bad[8+8+6]=1;add('has-false-nonzero-close-code',bytes(bad))
add('trailing-zero-word',b+bytes(8));add('trailing-byte',b+b'x')
add('oversize-input',b+bytes(65537-len(b)))
add('oversize-payload',envelope(1,b'x'*65536))
for n in (1,4,7,8,9,len(b)-1):add('truncated-%d'%n,b[:n])
add('segment-size-out-of-bounds',U(0)+U(100)+W(0))
add('unknown-segment-root',frame([W(far(5))]))
add('far-landing-out-of-range',frame([W(far(1,100)),W(0)]))
add('double-far-missing-second-word',frame([W(far(1,double=True)),W(0)]))
add('far-landing-is-far',frame([W(far(1)),W(far(0))]))
add('513-segments',frame([b'']*513))
add('payload-claims-out-of-bounds',frame([W(sp())+W(1)+W(lp(1,32))+W(lp(4,1000))+D]))
add('payload-is-struct',frame([W(sp())+W(1)+W(lp(1,32))+W(sp())+D+W(0)*3]))
add('payload-is-word-list',frame([W(sp())+W(1)+W(lp(1,32))+W(1|(4<<2)|(5<<32)|(1<<35))+D+W(0)]))
add('digest-is-capability',frame([W(sp())+W(1)+W(3)+W(0)]))
add('extra-reachable-capability',frame([W(sp(ps=3))+W(1)+W(lp(2,32))+W(0)+W(3)+D]))
# Valid single far root and independent far Data pointers, all four segments used.
p=b'far\x00\xff';dd=W(1|(1<<16))
multi=frame([W(far(1)),W(sp())+dd+W(far(2))+W(far(3)),W(lp(0,32))+D,W(lp(0,len(p)))+data(p)])
add('valid-four-segment-single-far',multi,True,1,p)
# Double-far landing pad names the start of a compatible struct in segment 2.
p='多段'.encode();seg2=W(1)+W(lp(1,32))+W(lp(4,len(p)))+D+data(p)
add('valid-double-far-root',frame([W(far(1,double=True)),W(far(2))+W(sp()),seg2]),True,0,p)
add('valid-unused-empty-segment',frame([b[8:],b'']),True)
add('valid-backward-data-pointer',frame([W(sp(off=4))+D+W(1|(1<<16))+W(lp(-6,32))+W(lp(0,3))+data(b'back'[:3])]),True,1,b'bac')
add('valid-unknown-pointer-data-alias',frame([W(sp(ps=3))+W(1)+W(lp(2,32))+W(0)+W(lp(0,32))+D]),True)
add('cyclic-unknown-struct-pointer',frame([W(sp(ps=3))+W(1)+W(lp(2,32))+W(0)+W(sp(off=-1,ds=0,ps=1))+D]))
# All unknown pointer fields are traversed by total_size(), even if ignored by schema.
ptrs=22;di=2+ptrs;alias_idx=di+4
words=[sp(ps=ptrs),1,lp(di-3,32),0]+[lp(alias_idx-i-1,8192) for i in range(4,2+ptrs)]
add('aliased-traversal-budget',frame([b''.join(map(W,words))+D+bytes(8192)]))
# Nine unknown nested structs, each with a pointer field, exceed nesting limit eight.
words=[sp(ps=3),1,lp(2,32),0,sp(off=4,ds=0,ps=1)]+list(struct.unpack('<4Q',D))+[sp(ds=0,ps=1)]*9+[0]
add('nested-unknown-pointer-budget',frame([b''.join(map(W,words))]))
root=Path(__file__).resolve().parent
binary=b'WSVC0002'+U(len(cases))
rows=[]
for x in cases:
 name=x['name'].encode();p=x['payload'];w=x['wire']
 binary+=struct.pack('<8I',len(name),int(x['accept']),int(x['encode_success']),x['kind'],x['has_close_code'],x['close_code'],len(p),len(w))+name+p+w
 rows.append({k:v for k,v in x.items() if k not in ('payload','wire')}|{'payload_hex':p.hex(),'wire_bytes':len(w),'wire_sha256':hashlib.sha256(w).hexdigest()})
(root/'vectors.bin').write_bytes(binary)
(root/'vectors.json').write_text(json.dumps({'format':'WSVC0002','vectors':rows,'expected_verdicts_require_native_oracle':True},indent=2)+'\n',encoding='utf8')
print(json.dumps({'vectors':len(cases),'accept':sum(x['accept'] for x in cases),'reject':sum(not x['accept'] for x in cases),'sha256':hashlib.sha256(binary).hexdigest()}))
