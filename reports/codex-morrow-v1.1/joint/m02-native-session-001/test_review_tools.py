"""Small negative checks for reviewer fail-closed behavior; no host/client run."""
import copy
import sys
sys.dont_write_bytecode=True
from check_review import HERE, JOINT, sha, read, write, verify_pins, check_matrix
from wire_review import authority, decode

def main():
    run=HERE/'runs'/'reviewer-negative-001'
    run.mkdir(exist_ok=False)
    cases=[]
    path=run/'fixture.txt'; path.write_text('fixed reviewer fixture',encoding='utf-8')
    pin={'path':str(path),'sha256':sha(path)}
    assert verify_pins([pin])[1]==[]
    path.write_text('modified reviewer fixture',encoding='utf-8')
    assert verify_pins([pin])[1][0]['error']=='sha256_mismatch'
    cases.append('changed_identity_rejected')
    missing={'path':str(run/'absent'),'sha256':'0'*64}
    assert verify_pins([missing])[1][0]['error']=='FileNotFoundError'
    cases.append('missing_identity_rejected')
    matrix=read(HERE/'matrix.json'); cat=read(JOINT/'index.json')
    matrix['diagnostics'][0]['status']='verified'
    assert any('preflight_cannot_grant_runtime_credit' in s for s in check_matrix(matrix,cat))
    cases.append('preflight_runtime_credit_rejected')
    schema,_=authority()
    raw=bytearray(schema['frame_bytes'])
    for offset,size,name,spec in schema['fields']:
        if name=='magic': raw[offset:offset+size]=spec.encode('ascii')
        elif isinstance(spec,int): raw[offset:offset+size]=spec.to_bytes(size,'little')
        elif name=='kind': raw[offset:offset+size]=int(spec.split()[0].split('=')[1]).to_bytes(size,'little')
    # This is a synthetic decoder fixture, never a valid admitted runtime session.
    assert decode(raw,schema)['kind_name']=='Challenge'
    for n in range(len(raw)):
        try: decode(raw[:n],schema)
        except ValueError: pass
        else: raise AssertionError(f'truncation accepted:{n}')
    cases.append('all_frame_truncations_rejected')
    for field in ['magic','major','kind','frame_bytes','reserved']:
        bad=bytearray(raw)
        off=next(f[0] for f in schema['fields'] if f[2]==field)
        bad[off]=255
        try: decode(bad,schema)
        except ValueError: pass
        else: raise AssertionError('invalid field accepted:'+field)
    cases.append('malformed_header_kind_reserved_rejected')
    write(run/'result.json',{'status':'verified_reviewer_negative_checks_only','cases':cases,
      'synthetic_decoder_fixture':True,'real_ipc_executed':False,'product_pass_credit':0,
      'test_sha256':sha(__file__)})
    print(f'{len(cases)} reviewer checks passed; no real IPC or product acceptance credit.')

if __name__=='__main__': main()
