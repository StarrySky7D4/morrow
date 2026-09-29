"""Independent read-only consumer of the host-owned JSON field table.

No locally authored schema, admission constructor, client or host launcher.
"""
import argparse
import json
import sys
from pathlib import Path
sys.dont_write_bytecode = True
from check_review import HERE, HOST, sha, read, write, verify_pins

HANDOFF = HOST / 'reports/codex-morrow-v1.1/host/m02-native-session-001/wire-handoff.json'
HANDOFF_SHA = '62dc56b74170096d74b51125c87a617bea98effd33686338598d19b2b78a1e8a'
SCHEMA_SHA = '40d87f6a5dc07c134faef67a2767429531a292f4e85f22f3c53eb6296b63001a'

def authority():
    if sha(HANDOFF) != HANDOFF_SHA:
        raise ValueError('wire handoff changed: a new explicit review is required')
    h = read(HANDOFF)
    pins = [{'path':str(HOST / p),'sha256':s} for p,s in h['input_sha256'].items()]
    results, issues = verify_pins(pins)
    if issues:
        raise ValueError(issues)
    schema_path = Path(h['authority']) / 'native_session_wire.json'
    if sha(schema_path) != SCHEMA_SHA or h['schema_sha256'] != SCHEMA_SHA:
        raise ValueError('wrong authoritative schema identity')
    schema = read(schema_path)
    if schema['byte_order'] != 'little endian':
        raise ValueError('unsupported authoritative byte order')
    coverage = []
    for offset,size,name,spec in schema['fields']:
        coverage.extend(range(offset,offset+size))
    if sorted(coverage) != list(range(schema['frame_bytes'])):
        raise ValueError('schema fields overlap or leave gaps')
    return schema, results

def decode(raw, schema):
    if len(raw) != schema['frame_bytes']:
        raise ValueError('incomplete_or_oversized_frame')
    out = {}
    for offset,size,name,spec in schema['fields']:
        field = raw[offset:offset+size]
        if name == 'magic':
            if field != spec.encode('ascii'):
                raise ValueError('magic')
            out[name] = spec
        elif name == 'reserved':
            if any(field):
                raise ValueError('reserved')
            out[name] = field.hex()
        elif name == 'nonce' or name.endswith('_sha256'):
            out[name] = field.hex()
        else:
            out[name] = int.from_bytes(field, 'little')
            if isinstance(spec,int) and out[name] != spec:
                raise ValueError('fixed_field:' + name)
            if name == 'kind':
                kinds = {int(part.split('=')[1]):part.split('=')[0] for part in spec.split()}
                if out[name] not in kinds:
                    raise ValueError('unknown_kind')
                out['kind_name'] = kinds[out[name]]
    # Decoding does not by itself assert approval, direction or session validity.
    return out

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--run-id', required=True)
    ap.add_argument('--frame', type=Path)
    args=ap.parse_args()
    import re
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,90}',args.run_id):
        ap.error('unsafe run id')
    run=HERE/'runs'/args.run_id
    run.parent.mkdir(exist_ok=True); run.mkdir(exist_ok=False)
    result={'scope':'host_wire_identity_and_consumer_decode_only', 'independent_runtime':False,
            'product_pass_credit':0, 'handoff_sha256':HANDOFF_SHA,'schema_sha256':SCHEMA_SHA,
            'reviewer_sha256':sha(__file__)}
    try:
        schema,pins=authority()
        result.update(status='verified_wire_inputs_only',input_count=len(pins),issues=[])
        write(run/'identities.json',pins)
        if args.frame:
            result['frame']={'path':str(args.frame.resolve()),'sha256':sha(args.frame),
                             'decoded':decode(args.frame.read_bytes(),schema)}
        result['exit_code']=0
    except Exception as exc:
        result.update(status='failed',issues=[{'error':type(exc).__name__,'message':str(exc)}],exit_code=1)
    write(run/'result.json',result)
    print(json.dumps(result,ensure_ascii=False,indent=2))
    return result['exit_code']

if __name__=='__main__':
    raise SystemExit(main())
