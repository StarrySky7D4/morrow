#!/usr/bin/env python3
"""Independent deterministic canonical and malformed wire fixtures (stdlib only)."""
import hashlib,json,struct
from pathlib import Path
ROOT=Path(__file__).resolve().parent
PROFILE=bytes.fromhex('07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089')
DOMAIN=b'Morrow/changes-metadata/cursor/v1\0'
def cursor(payload):return hashlib.sha256(DOMAIN+payload).digest()
def payload(card=b'card-a',op=b'op-1'):
 return b'MRCHG001'+struct.pack('<H',1)+PROFILE+bytes([1])*32+bytes([2])*32+struct.pack('<Q',42)+bytes([3])*32+struct.pack('<HH',len(card),len(op))+card+op
cases=[]
def add(name,p,valid=False,epoch=bytes([2])*32,cur=None,clen=32):
 cases.append((name,valid,epoch,clen,cursor(p) if cur is None else cur,p))
base=payload()
add('canonical',base,True)
add('minimum-identities',payload(b'a',b'b'),True)
add('maximum-identities',payload(b'a'*256,b'b'*256),True)
add('unicode-identities',payload('卡片🦉'.encode(),'变更é'.encode()),True)
add('byte-preservation-spaces',payload(b' card ',b' op '),True)
add('unicode-max-256-bytes',payload(('é'*128).encode(),('🦉'*64).encode()),True)
for n in range(len(base)):add('truncated-'+str(n),base[:n])
for name,offset,value in [('magic',0,b'X'),('version-zero',8,b'\0\0'),('version-future',8,b'\2\0'),('profile',10,b'\0'*32),('scope-zero',42,b'\0'*32),('window-zero',74,b'\0'*32),('revision-zero',106,b'\0'*8),('empty-card-length',146,b'\0\0'),('empty-operation-length',148,b'\0\0'),('too-long-card',146,b'\1\1'),('too-long-operation',148,b'\1\1'),('big-endian-lengths',146,b'\0\6\0\4')]:
 p=bytearray(base);p[offset:offset+len(value)]=value;add(name,bytes(p))
for name,bad in [('slash',b'a/b'),('backslash',b'a\\b'),('colon',b'a:b'),('nul',b'a\0b'),('ascii-control',b'a\x1fb'),('delete',b'a\x7fb'),('c1-control','a\u0085b'.encode()),('invalid-utf8',b'\xff'),('overlong-utf8',b'\xc0\xaf'),('surrogate',b'\xed\xa0\x80'),('beyond-unicode',b'\xf4\x90\x80\x80')]:
 add('card-'+name,payload(bad,b'op'));add('operation-'+name,payload(b'card',bad))
add('card-257-bytes',payload(b'a'*257,b'o'));add('operation-257-bytes',payload(b'c',b'o'*257))
add('trailing',base+b'\0');add('oversized',base+b'\0'*700)
add('wrong-epoch',base,epoch=bytes([4])*32);add('wrong-cursor',base,cur=bytes(32));add('cursor-short',base,clen=31);add('cursor-long',base,clen=33)
add('cardhash-not-covered',base[:114]+bytes([7])*32+base[146:],cur=cursor(base))
blob=b''.join(bytes([ok])+epoch+struct.pack('<I',clen)+cur+struct.pack('<I',len(p))+p for _,ok,epoch,clen,cur,p in cases)
(ROOT/'vectors.bin').write_bytes(blob)
(ROOT/'vectors.json').write_text(json.dumps({'format':'repeat: accept:u8 epoch:32 cursor_length:u32le cursor:32 payload_length:u32le payload:bytes','sha256':hashlib.sha256(blob).hexdigest(),'cases':[{'name':n,'accept':ok,'payload_sha256':hashlib.sha256(p).hexdigest()} for n,ok,_,_,_,p in cases]},indent=2)+'\n')
print(len(cases),'vectors;',sum(c[1] for c in cases),'accepted;',len(blob),'bytes')
