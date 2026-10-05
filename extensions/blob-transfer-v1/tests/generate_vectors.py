"""Independent wire fixtures, not a production decoder or runtime proof."""
import hashlib, json, struct
from pathlib import Path
W=lambda x:struct.pack('<Q',x)
U=lambda x:struct.pack('<I',x)
def pad(x):return x+b'\0'*((-len(x))%8)
def sp(off=0,ds=1,ps=5):return ((off&0x3fffffff)<<2)|(ds<<32)|(ps<<48)
def lp(off,n):return 1|((off&0x3fffffff)<<2)|(2<<32)|(n<<35)
def far(seg,off=0,double=False):return 2|(int(double)<<2)|(off<<3)|(seg<<32)
def frame(segs):
 h=U(len(segs)-1)+b''.join(U(len(s)//8)for s in segs)
 return pad(h)+b''.join(segs)
root=Path(__file__).resolve().parent
D=hashlib.sha256((root.parent/'contracts/blob_transfer.capnp').read_bytes()).digest()
def action(kind=0,payload=b'abc',seq=1,offset=0,total=3,whole=None,status=0):
 whole=whole if whole is not None else hashlib.sha256(payload).digest()
 if kind in (0,3):return W(sp(ds=1,ps=1))+W(total)+W(lp(0,32))+whole
 if kind==1:return W(sp(ds=2,ps=2))+W(seq)+W(offset)+W(lp(1,len(payload)))+W(lp(len(pad(payload))//8,32))+pad(payload)+whole
 return W(sp(ds=4,ps=2))+W(seq)+W(offset)+W(total)+W(status)+W(lp(1,32))+W(lp(4,32))+bytes([9])*32+whole
def envelope(kind=0,payload=b'abc',seq=1,offset=0,total=3,whole=None,status=0,version=1,digest=D,epoch=bytes([1])*32,ref=bytes([2])*32,op=bytes([3])*32,ds=1,ps=5):
 a=action(kind,payload,seq,offset,total,whole,status)
 fields=[digest,epoch,ref,op];idx=1+ds+ps;targets=[];chunks=[]
 for x in fields:targets.append(idx);chunks.append(pad(x));idx+=len(pad(x))//8
 ai=idx
 words=[sp(ds=ds,ps=ps),version|(kind<<16)]+[0]*(ds-1)
 words += [lp(t-(1+ds+i)-1,len(x))for i,(t,x)in enumerate(zip(targets,fields))]
 # Action struct pointer, formerly word0 of standalone action, is relocated.
 words += [sp(ai-(1+ds+4)-1,ds=1 if kind in(0,3)else 2 if kind==1 else 4,ps=1 if kind in(0,3)else 2)]
 words += [0]*(ps-5)
 return frame([b''.join(map(W,words))+b''.join(chunks)+a[8:]])
rows=[]
def add(name,wire,ok=False,kind=0,payload=b'abc',seq=1,offset=0,total=3,status=0,whole=None):
 whole=whole if whole is not None else hashlib.sha256(payload).digest()
 rows.append(dict(name=name,wire=wire,accept=ok,kind=kind,payload=payload if kind==1 else b'',seq=seq if kind in(1,2)else 0,offset=offset if kind in(1,2)else 0,total=total if kind in(0,2,3)else len(payload),status=status,whole=whole))
def good(name,kind=0,**kw):add(name,envelope(kind=kind,**kw),True,kind=kind,**{k:v for k,v in kw.items()if k in('payload','seq','offset','total','status','whole')})
for k in range(4):good('action-%d'%k,k)
good('receipt-existing',2,status=1)
good('empty-descriptor',total=0,payload=b'')
good('empty-chunk',1,payload=b'')
good('chunk-60k',1,payload=bytes(range(256))*240)
good('chunk-last-byte',1,offset=16*1024*1024-3)
good('descriptor-16m',total=16*1024*1024)
good('receipt-max-sequence',2,seq=2**64-1)
good('expanded-compatible',ds=3,ps=7)
base=envelope()
for v in(0,2,65535):add('version-%d'%v,envelope(version=v))
for dg,n in((b'','missing'),(D[:31],'short'),(D+b'x','long'),(bytes(32),'wrong')):add('digest-'+n,envelope(digest=dg))
for key in('epoch','ref','op'):
 for x,n in((bytes(32),'zero'),(bytes(31),'short'),(bytes(33),'long')):add(key+'-'+n,envelope(**{key:x}))
add('unknown-union',envelope(kind=4))
add('unknown-status',envelope(kind=2,status=2))
for k in(1,2):
 add('sequence-zero-%d'%k,envelope(kind=k,seq=0))
 add('overflow-range-%d'%k,envelope(kind=k,offset=2**64-1))
for k in(0,2,3):add('length-limit-%d'%k,envelope(kind=k,total=16*1024*1024+1 if k!=2 else 60*1024+1))
add('chunk-60k-plus-one',envelope(kind=1,payload=b'x'*(60*1024+1)))
add('wrong-chunk-sha',envelope(kind=1,whole=bytes(32)))
for n in(0,1,4,7,8,9,len(base)-1):add('truncated-%d'%n,base[:n])
add('tail-byte',base+b'x');add('tail-word',base+bytes(8))
add('wire-cap',base+bytes(65537-len(base)))
add('null-root',frame([W(0)]));add('null-action',base[:8+6*8]+W(0)+base[8+7*8:])
add('capability-root',frame([W(3)]))
add('far-unknown-segment',frame([W(far(9))]))
add('far-oob',frame([W(far(1,99)),W(0)]))
add('double-far-short',frame([W(far(1,double=True)),W(0)]))
add('far-landing-far',frame([W(far(1)),W(far(0))]))
add('513-segments',frame([b'']*513))
add('valid-single-far',frame([W(far(1)),base[8:]]),True)
add('valid-double-far',frame([W(far(1,double=True)),W(far(2))+W(sp()),base[16:]]),True)
add('valid-unused-empty-segment',frame([base[8:],b'']),True)
# Expanded root exposes reachable unknown capability/cycle; caps are not fields.
for value,n in((3,'unknown-capability'),(sp(-1,0,1),'unknown-cycle')):
 w=bytearray(envelope(ds=1,ps=6));struct.pack_into('<Q',w,8+7*8,value);add(n,bytes(w))
binary=b'BLCV0001'+U(len(rows));inventory=[]
for r in rows:
 name=r['name'].encode();payload=r['payload'];wire=r['wire']
 binary+=struct.pack('<6I4Q',len(name),int(r['accept']),len(wire),r['kind'],len(payload),r['status'],r['seq'],r['offset'],r['total'],0)+r['whole']+name+payload+wire
 inventory.append({k:v for k,v in r.items()if k not in('payload','wire','whole')}|{'payload_hex':payload.hex()if len(payload)<128 else None,'payload_sha256':hashlib.sha256(payload).hexdigest(),'whole_sha256_hex':r['whole'].hex(),'wire_bytes':len(wire),'wire_sha256':hashlib.sha256(wire).hexdigest()})
(root/'vectors.bin').write_bytes(binary)
(root/'vectors.json').write_text(json.dumps({'format':'BLCV0001','scope':'Independent expected schema semantics; no production receiver/import qualification','vectors':inventory},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'vectors':len(rows),'accept':sum(r['accept']for r in rows),'reject':sum(not r['accept']for r in rows),'sha256':hashlib.sha256(binary).hexdigest()}))
