"""Bounded reader/test-mutation helper for host-local MRNADM01 records.

Not a guest wire or authority implementation. Mutations only target this review's
temporary database after the actual trusted host has created an approval.
"""
import hashlib
def varint(n):
    if not 0<=n<2**64:raise ValueError('varint range')
    out=bytearray()
    while n>=128:out.append((n&127)|128);n>>=7
    out.append(n);return bytes(out)
def take_varint(data,i):
    n=0
    for shift in range(0,70,7):
        if i>=len(data):raise ValueError('truncated varint')
        b=data[i];i+=1;n|=(b&127)<<shift
        if b<128:
            if n>=2**64:raise ValueError('varint overflow')
            return n,i
    raise ValueError('long varint')
def raw_fields(raw):
    fields={};i=0
    while i<len(raw):
        key,i=take_varint(raw,i);number,wire=key>>3,key&7
        if number==0 or number in fields:raise ValueError('invalid/duplicate field')
        if wire==0:value,i=take_varint(raw,i)
        elif wire==2:
            size,i=take_varint(raw,i)
            if i+size>len(raw):raise ValueError('truncated field')
            value=raw[i:i+size];i+=size
        else:raise ValueError('unexpected native record wire type')
        fields[number]=(wire,value)
    return fields
def encode_fields(fields):
    out=bytearray()
    for number,(wire,value) in sorted(fields.items()):
        out.extend(varint(number*8+wire))
        if wire==0:out.extend(varint(value))
        elif wire==2:out.extend(varint(len(value)));out.extend(value)
        else:raise ValueError('unsupported field')
    return bytes(out)
def decompress(data,expected):
    out=bytearray();i=0
    def length(initial,index):
        n=initial
        if n==15:
            while True:
                if index>=len(data):raise ValueError('truncated LZ4 length')
                b=data[index];index+=1;n+=b
                if b!=255:break
        return n,index
    while i<len(data):
        token=data[i];i+=1;n,i=length(token>>4,i)
        if i+n>len(data) or len(out)+n>expected:raise ValueError('LZ4 literal bounds')
        out.extend(data[i:i+n]);i+=n
        if i==len(data):break
        if i+2>len(data):raise ValueError('LZ4 offset truncated')
        offset=int.from_bytes(data[i:i+2],'little');i+=2
        if not 0<offset<=len(out):raise ValueError('LZ4 invalid offset')
        n,i=length(token&15,i);n+=4
        if len(out)+n>expected:raise ValueError('LZ4 output limit')
        for _ in range(n):out.append(out[-offset])
    if len(out)!=expected:raise ValueError('LZ4 output length')
    return bytes(out)
def unpack(blob):
    if len(blob)<50 or len(blob)>8352 or blob[:10]!=b'MRNADM01\x01\x00':raise ValueError('envelope identity')
    n=int.from_bytes(blob[10:14],'little');size=int.from_bytes(blob[14:18],'little')
    if n>8192 or size!=len(blob)-50:raise ValueError('envelope length')
    raw=decompress(blob[50:],n)
    if hashlib.sha256(raw).digest()!=blob[18:50]:raise ValueError('envelope digest')
    fields=raw_fields(raw)
    if encode_fields(fields)!=raw:raise ValueError('noncanonical protobuf')
    return fields
def pack_literal(fields):
    raw=encode_fields(fields)
    if len(raw)>8192:raise ValueError('record limit')
    # Valid LZ4 block consisting solely of final literals. No fake runtime frames.
    encoded=bytearray([min(15,len(raw))<<4]);n=len(raw)-15
    if n>=0:
        while n>=255:encoded.append(255);n-=255
        encoded.append(n)
    encoded.extend(raw)
    return b'MRNADM01\x01\x00'+len(raw).to_bytes(4,'little')+len(encoded).to_bytes(4,'little')+hashlib.sha256(raw).digest()+encoded
