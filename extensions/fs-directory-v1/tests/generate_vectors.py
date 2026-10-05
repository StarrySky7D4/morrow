"""Independent fs-directory schema fixtures; not a native listing oracle."""
import hashlib,json,struct
from pathlib import Path
W=lambda x:struct.pack('<Q',x)
U=lambda x:struct.pack('<I',x)
def pad(x):return x+b'\0'*((-len(x))%8)
def sp(off=0,ds=2,ps=3):return((off&0x3fffffff)<<2)|(ds<<32)|(ps<<48)
def lp(off,n):return 1|((off&0x3fffffff)<<2)|(2<<32)|(n<<35)
def il(off,words):return 1|((off&0x3fffffff)<<2)|(7<<32)|(words<<35)
def far(seg,off=0,double=False):return 2|(int(double)<<2)|(off<<3)|(seg<<32)
def frame(segs):return pad(U(len(segs)-1)+b''.join(U(len(s)//8)for s in segs))+b''.join(segs)
root=Path(__file__).resolve().parent
D=hashlib.sha256((root.parent/'contracts/fs_directory.capnp').read_bytes()).digest()
def entry(name=b'a',encoding=1,kind=1,length=None,i=2):return dict(name=name,encoding=encoding,kind=kind,length=length,id=bytes([i])*32)
def envelope(entries=None,seq=1,terminal=True,epoch=bytes([1])*32,digest=D,version=1,ds=2,ps=3,alias=False,raw_lengths=None):
 entries=[entry()]if entries is None else entries
 fixed=1+ds+ps;di=fixed;ei=di+len(pad(digest))//8;li=ei+len(pad(epoch))//8
 words=[sp(ds=ds,ps=ps),version|(int(terminal)<<16),seq]+[0]*(ds-2)
 words += [lp(di-(1+ds)-1,len(digest)),lp(ei-(2+ds)-1,len(epoch)),il(li-(3+ds)-1,4*len(entries))]+[0]*(ps-3)
 parts=b''.join(map(W,words))+pad(digest)+pad(epoch)+W((len(entries)<<2)|(2<<32)|(2<<48))
 idx=li+1+4*len(entries);ew=[];chunks=[];first_name=None
 for j,e in enumerate(entries):
  target_id=idx;chunks.append(e['id']);idx+=4
  if alias and first_name is not None:target_name=first_name
  else:target_name=idx;first_name=idx;chunks.append(pad(e['name']));idx+=len(pad(e['name']))//8
  pos=li+1+4*j
  bits=e['encoding']|(e['kind']<<16)|(int(e['length']is not None)<<32)
  ew += [W(bits),W(e['length']or 0 if raw_lengths is None else raw_lengths[j]),W(lp(target_id-(pos+2)-1,len(e['id']))),W(lp(target_name-(pos+3)-1,len(e['name'])))]
 return frame([parts+b''.join(ew)+b''.join(chunks)])
rows=[]
def add(name,wire,ok=False,entries=None,seq=1,terminal=True):rows.append(dict(name=name,wire=wire,accept=ok,entries=[entry()]if entries is None else entries,seq=seq,terminal=terminal))
def good(name,entries=None,**kw):add(name,envelope(entries,**kw),True,entries,**{k:v for k,v in kw.items()if k in('seq','terminal')})
good('file-no-length');good('file-zero', [entry(length=0)]);good('file-max-length',[entry(length=2**64-1)])
good('utf8-unicode',[entry('雪🙂'.encode())]);good('utf16-isolated-surrogate',[entry(b'\x00\xd8x\x00',2)])
good('directory',[entry(kind=2)]);good('other',[entry(kind=3)]);good('empty-terminal',[])
good('max-sequence',seq=2**64-1);good('expanded-compatible',ds=4,ps=5)
good('32-entries',[entry(b'x',i=i+2)for i in range(32)])
good('16k-names',[entry(b'x'*8192,i=2),entry(b'y'*8192,i=3)])
good('valid-shared-data',[entry(b'x'*4096,i=2),entry(b'x'*4096,i=3)],alias=True)
base=envelope()
for version in(0,2,65535):add('version-%d'%version,envelope(version=version))
for digest,n in((b'','missing'),(D[:31],'short'),(D+b'x','long'),(bytes(32),'wrong')):add('digest-'+n,envelope(digest=digest))
for epoch,n in((bytes(32),'zero'),(bytes(31),'short'),(bytes(33),'long')):add('epoch-'+n,envelope(epoch=epoch))
add('sequence-zero',envelope(seq=0));add('33-entries',envelope([entry(i=i+2)for i in range(33)]))
add('16k-plus-one-names',envelope([entry(b'x'*8193,i=2),entry(b'y'*8192,i=3)]))
add('shared-data-aggregate-over-limit',envelope([entry(b'x'*10000,i=2),entry(b'x'*10000,i=3)],alias=True))
add('duplicate-id',envelope([entry(),entry(b'b')]))
for name,encoding in((b'',1),(b'.',1),(b'..',1),(b'a\0b',1),(b'a/b',1),(b'a\\b',1),(b'\xff',1),(b'\xc0\xaf',1),(b'\xed\xa0\x80',1),(b'x',2),(b'\0\0',2),(b'.\0',2),(b'/\0',2),(b'\\\0',2)):
 add('invalid-name-'+name.hex()+'-'+str(encoding),envelope([entry(name,encoding)]))
for enc in(0,3,65535):add('encoding-%d'%enc,envelope([entry(encoding=enc)]))
for kind in(0,4,65535):add('kind-%d'%kind,envelope([entry(kind=kind)]))
for kind in(2,3):add('nonfile-length-%d'%kind,envelope([entry(kind=kind,length=0)]))
add('false-length-with-nonzero',envelope(raw_lengths=[1]))
for ident,n in((bytes(32),'zero'),(bytes(31),'short'),(bytes(33),'long')):
 e=entry();e['id']=ident;add('entry-id-'+n,envelope([e]))
for n in(0,1,4,7,8,9,len(base)-1):add('truncated-%d'%n,base[:n])
add('tail-byte',base+b'x');add('tail-word',base+bytes(8));add('wire-cap',base+bytes(65537-len(base)))
add('null-root',frame([W(0)]));add('capability-root',frame([W(3)]))
add('far-unknown-segment',frame([W(far(9))]));add('far-oob',frame([W(far(1,99)),W(0)]))
add('double-far-short',frame([W(far(1,double=True)),W(0)]));add('far-landing-far',frame([W(far(1)),W(far(0))]));add('513-segments',frame([b'']*513))
add('valid-single-far',frame([W(far(1)),base[8:]]),True)
add('valid-double-far',frame([W(far(1,double=True)),W(far(2))+W(sp()),base[16:]]),True)
add('valid-unused-empty-segment',frame([base[8:],b'']),True)
for value,n in((3,'unknown-capability'),(sp(-1,0,1),'unknown-cycle')):
 w=bytearray(envelope(ps=4));struct.pack_into('<Q',w,8+6*8,value);add(n,bytes(w))
binary=b'DRCV0001'+U(len(rows));inventory=[]
for r in rows:
 name=r['name'].encode();wire=r['wire'];binary+=struct.pack('<6IQ',len(name),int(r['accept']),len(wire),len(r['entries']),int(r['terminal']),0,r['seq'])+name
 for e in r['entries']:binary+=e['id']+struct.pack('<4IQ',len(e['name']),e['encoding'],e['kind'],int(e['length']is not None),e['length']or 0)+e['name']
 binary+=wire
 inventory.append({'name':r['name'],'accept':r['accept'],'entries':len(r['entries']),'sequence':r['seq'],'terminal':r['terminal'],'wire_bytes':len(wire),'wire_sha256':hashlib.sha256(wire).hexdigest()})
(root/'vectors.bin').write_bytes(binary);(root/'vectors.json').write_text(json.dumps({'format':'DRCV0001','scope':'Independent schema semantics, not OS enumeration/grant/SDK freeze','vectors':inventory},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'vectors':len(rows),'accept':sum(r['accept']for r in rows),'reject':sum(not r['accept']for r in rows),'sha256':hashlib.sha256(binary).hexdigest()}))
