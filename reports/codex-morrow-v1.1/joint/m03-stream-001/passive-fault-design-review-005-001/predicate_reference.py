"""Pure, synthetic design checks. Not the producer harness or runtime proof.

Facts must later be extracted from pinned original evidence. A true input flag
here never replaces its underlying OS/frame/ledger/source evidence.
"""
from pathlib import Path
import copy, hashlib, json, struct

J=Path(__file__).resolve().parent
H='host-authority-monotonic:17267034834078408336:1'
N='1'*64

def digest(raw): return hashlib.sha256(raw).hexdigest()
def canonical_identity(identity):
    keys=('session','epoch','child_pid','attempt','operation_id_sha256','host_execution_config_sha256')
    assert set(identity)==set(keys)
    for k in keys[:4]:
        assert type(identity[k]) is int and 0<identity[k]<(1<<64)
    for k in keys[4:]:
        assert len(identity[k])==64 and set(identity[k])<=set('0123456789abcdef')
    return json.dumps({k:identity[k] for k in keys},separators=(',',':')).encode('utf-8')

def expiry(v):
    d=v['deadline_ns'];t=v['challenge_sample_ns']
    return {
      'single_bound_original_deadline':v['created_ns']==0 and d==v['ttl_ms']*1_000_000
          and v['parent_deadline_ns']==v['gate_deadline_ns']==v['send_deadline_ns']==d and not v['deadline_changed'],
      'same_host_domain':v['deadline_domain']==v['expiry_domain']==v['challenge_domain']==H,
      'actual_challenge_sample':0<=t<d and v['remaining_ms']==(d-t)//1_000_000
          and v['raw_challenge_remaining_ms']==v['remaining_ms'] and t<=v['challenge_sent_ns']<d,
      'real_head_and_consumption':v['head_accepted'] and v['actual_delta_delivered'] and v['parser_bytes']>0,
      'first_natural_expiry':v['expiry_ns']>=d and v['requested_code']==v['effective_first_reason']==20
          and v['first_source']==2 and not v['earlier_disqualifier'] and not v['manual_fault'],
      'server_not_the_trigger':v['server_was_kept_open'] and v['cleanup_clock']==v['expiry_receipt_clock']
          and v['server_cleanup_ns']>=v['expiry_receipt_ns'],
      'no_success_relabel':not v['http_eof'] and not v['material'] and not v['completed_sent'],
    }

def network_fin(v):
    return {
      'unfinished_fixed_length':v['content_length']==8192 and 0<v['body_bytes']<1024
          and not v['transfer_encoding'] and not v['completed_sent'],
      'real_consumption_before_close':v['valid_delta_marker'] and v['valid_parser_marker'] and v['parser_bytes']>0
          and v['harness_clock']==v['delta_receipt_clock']==v['parser_receipt_clock']
          and max(v['delta_receipt_ns'],v['parser_receipt_ns'])<=v['close_before_ns']<=v['close_after_ns'],
      'same_owned_connection':v['post_count']==1 and v['post_connection']==v['closed_connection']
          and v['shutdown_ok'] and v['locked_close_method']==v['close_method']
          and v['close_method'] in ('shutdown(SHUT_WR)','shutdown(SHUT_RDWR)') and not v['rst_option'],
      'actual_transport_before_expiry':v['network_error']=='Transport' and v['error_domain']==H
          and v['network_error_ns']<=v['effect_close_ns']<v['deadline_ns'] and not v['earlier_disqualifier'],
      'first_transport_reason':v['first_source']==2 and v['requested_code']==v['effective_first_reason']==26,
      'no_success_relabel':not v['http_eof'] and not v['material'],
    }

def pipe_cut(v):
    full=bytes.fromhex(v['original_raw_hex']);prefix=bytes.fromhex(v['prefix_hex'])
    frags=[bytes.fromhex(x['bytes_hex']) for x in v['guest_read_fragments']]
    return {
      'real_original_body_frame':len(full)>12 and struct.unpack('<I',full[:4])[0]+4==len(full)
          and v['decoded_kind']=='BodyChunk' and v['first_body_target'] and v['decoded_tuple_matches']
          and digest(full)==v['original_frame_sha256'],
      'exact_prefix':len(prefix)==12 and prefix==full[:12] and digest(prefix)==v['prefix_sha256'],
      'actual_single_prefix_completion':v['requested']==v['completed']==12 and v['completion_error'] is None
          and v['issue_id']==v['reap_id'] and v['issue_ordinal']==v['reap_ordinal'] and v['reap_count']==1,
      'cumulative_guest_reads':b''.join(frags)==prefix and v['buffered_bytes']==12
          and v['expected_frame_bytes']==len(full) and v['declared_payload_bytes']==len(full)-4
          and all(x['error'] is None and x['transferred']==len(raw) for x,raw in zip(v['guest_read_fragments'],frags))
          and v['last_read_id']==v['guest_read_fragments'][-1]['id'] and v['read_issue_count']>0
          and v['io_replay_verified'] and v['valid_marker_envelope'],
      'witness_causally_closes_owner':v['prefix_reaped_before_match'] and v['witness_matched']
          and v['owner_closed'] and v['close_trigger']=='matched-witness'
          and v['close_evidence_domain']==H and v['owner_close_observed_ns']<v['deadline_ns']
          and not v['earlier_disqualifier'],
      'no_original_completion_or_tail':not v['original_frame_complete'] and not v['ordinary_write_completed']
          and not v['target_data_frame_sent'] and not v['tail_reissued']
          and v['http_issued']==v['http_os_completed']==v['http_peer_consumed']==v['guest_parser_bytes']==0,
      'partial_failure_preserved':v['worker_returned_error'] and v['sticky_data_failure']
          and v['data_failure_kind'] in ('partial-frame-eof','partial-frame-control-close'),
    }

def all_true(checks): return all(checks.values())
def limited_resource_acceptance(cause_checks, resources, observer_complete):
    # Missing settlement ACK may be an accurately observed production failure.
    # Missing resource evidence is never accepted as actual cleanup.
    return all_true(cause_checks) and observer_complete and all(resources.values())

E={'created_ns':0,'ttl_ms':10000,'deadline_ns':10_000_000_000,'parent_deadline_ns':10_000_000_000,
   'gate_deadline_ns':10_000_000_000,'send_deadline_ns':10_000_000_000,'deadline_changed':False,
   'deadline_domain':H,'expiry_domain':H,'challenge_domain':H,'challenge_sample_ns':1_000_000_123,
   'remaining_ms':8999,'raw_challenge_remaining_ms':8999,'challenge_sent_ns':1_100_000_000,
   'head_accepted':True,'actual_delta_delivered':True,'parser_bytes':197,'expiry_ns':10_000_000_001,
   'requested_code':20,'effective_first_reason':20,'first_source':2,'earlier_disqualifier':False,
   'manual_fault':False,'server_was_kept_open':True,'cleanup_clock':'harness','expiry_receipt_clock':'harness',
   'server_cleanup_ns':1100,'expiry_receipt_ns':1000,'http_eof':False,'material':False,'completed_sent':False}
F={'content_length':8192,'body_bytes':197,'transfer_encoding':False,'completed_sent':False,
   'valid_delta_marker':True,'valid_parser_marker':True,'parser_bytes':197,'harness_clock':'harness',
   'delta_receipt_clock':'harness','parser_receipt_clock':'harness','delta_receipt_ns':100,
   'parser_receipt_ns':110,'close_before_ns':120,'close_after_ns':130,'post_count':1,
   'post_connection':'accepted-1','closed_connection':'accepted-1','shutdown_ok':True,
   'locked_close_method':'shutdown(SHUT_WR)','close_method':'shutdown(SHUT_WR)','rst_option':False,
   'network_error':'Transport','error_domain':H,'network_error_ns':2_000_000_000,'effect_close_ns':2_000_000_001,
   'deadline_ns':10_000_000_000,'earlier_disqualifier':False,'first_source':2,'requested_code':26,
   'effective_first_reason':26,'http_eof':False,'material':False}
payload=b'SYNTHETIC DESIGN PAYLOAD, NOT A REAL CAPNP FRAME'*4
raw=struct.pack('<I',len(payload))+payload
Q={'original_raw_hex':raw.hex(),'decoded_kind':'BodyChunk','first_body_target':True,'decoded_tuple_matches':True,
   'original_frame_sha256':digest(raw),'prefix_hex':raw[:12].hex(),'prefix_sha256':digest(raw[:12]),
   'requested':12,'completed':12,'completion_error':None,'issue_id':27,'reap_id':27,
   'issue_ordinal':3,'reap_ordinal':3,'reap_count':1,
   'guest_read_fragments':[{'id':8,'transferred':4,'error':None,'bytes_hex':raw[:4].hex()},
                           {'id':11,'transferred':8,'error':None,'bytes_hex':raw[4:12].hex()}],
   'buffered_bytes':12,'expected_frame_bytes':len(raw),'declared_payload_bytes':len(payload),
   'last_read_id':11,'read_issue_count':5,'io_replay_verified':True,'valid_marker_envelope':True,
   'prefix_reaped_before_match':True,'witness_matched':True,'owner_closed':True,'close_trigger':'matched-witness',
   'close_evidence_domain':H,'owner_close_observed_ns':2_000_000_000,'deadline_ns':10_000_000_000,
   'earlier_disqualifier':False,'original_frame_complete':False,'ordinary_write_completed':False,
   'target_data_frame_sent':False,'tail_reissued':False,'http_issued':0,'http_os_completed':0,
   'http_peer_consumed':0,'guest_parser_bytes':0,'worker_returned_error':True,
   'sticky_data_failure':True,'data_failure_kind':'partial-frame-eof'}

if __name__=='__main__':
    results=[]
    for name,fn,v in [('natural-expiry',expiry,E),('short-length-fin',network_fin,F),('pipe-cut-cumulative-4+8',pipe_cut,Q)]:
        checks=fn(v);assert all_true(checks);results.append({'case':name,'accepted':True,'checks':checks})
    negatives=[
      ('expiry-rearmed',expiry,E,{'deadline_changed':True}),
      ('expiry-before-original-D',expiry,E,{'expiry_ns':9_999_999_999}),
      ('expiry-first19-not-overwritten',expiry,E,{'effective_first_reason':19}),
      ('expiry-cross-domain',expiry,E,{'expiry_domain':'pipe-owner'}),
      ('expiry-early-server-close',expiry,E,{'server_cleanup_ns':999}),
      ('expiry-manual-revoke',expiry,E,{'manual_fault':True}),
      ('expiry-challenge-rounding-mismatch',expiry,E,{'remaining_ms':9000}),
      ('fin-marker-before-real-consumption',network_fin,F,{'parser_bytes':0}),
      ('fin-expiry-wins',network_fin,F,{'effect_close_ns':10_000_000_000}),
      ('fin-close-wrong-connection',network_fin,F,{'closed_connection':'other'}),
      ('fin-rst-unapproved',network_fin,F,{'rst_option':True}),
      ('fin-ordinary-eof-is-not-transport',network_fin,F,{'network_error':'EOF','http_eof':True}),
      ('fin-observed-material',network_fin,F,{'material':True}),
      ('cut-operator-ack-only',pipe_cut,Q,{'owner_closed':False}),
      ('cut-short-success8-not12',pipe_cut,Q,{'completed':8}),
      ('cut-late-owner-close',pipe_cut,Q,{'owner_close_observed_ns':10_000_000_000}),
      ('cut-false-original-completion',pipe_cut,Q,{'ordinary_write_completed':True}),
      ('cut-tail-reissued',pipe_cut,Q,{'tail_reissued':True}),
      ('cut-12-wire-bytes-as-http',pipe_cut,Q,{'http_os_completed':12}),
      ('cut-partial-timeout-wins',pipe_cut,Q,{'data_failure_kind':'partial-frame-timeout'}),
      ('cut-data-error-hidden-by-unknown',pipe_cut,Q,{'sticky_data_failure':False}),
      ('cut-marker-not-real-IO',pipe_cut,Q,{'io_replay_verified':False}),
      ('cut-wrong-last-read-id',pipe_cut,Q,{'last_read_id':1}),
      ('cut-cross-domain-close',pipe_cut,Q,{'close_evidence_domain':'guest-fixture-monotonic'}),
    ]
    for name,fn,base,change in negatives:
        v=copy.deepcopy(base);v.update(change);checks=fn(v);assert not all_true(checks),name
        results.append({'case':name,'accepted':False,'rejected_predicates':[k for k,b in checks.items() if not b]})
    resources={k:True for k in ('host_pipe_join','host_network_join','guest_data_join','guest_control_joins',
        'evidence_writer_join','server_threads_join','child_wait_exit','stdout_eof','stderr_eof','owner_released')}
    assert limited_resource_acceptance(expiry(E),resources,True)
    incomplete=resources.copy();incomplete['guest_data_join']=False
    assert not limited_resource_acceptance(expiry(E),incomplete,True)
    assert not limited_resource_acceptance(expiry(E),resources,False)
    results.extend([{'case':'expiry-missing-settlement-is-separate-from-resource-proof','accepted':True},
                    {'case':'resource-missing-guest-join','accepted':False},
                    {'case':'observer-incomplete','accepted':False}])
    identity={'session':17267034834078408336,'epoch':1,'child_pid':25812,'attempt':1,
              'operation_id_sha256':N,'host_execution_config_sha256':'2'*64}
    encoded=canonical_identity(identity)
    assert str(identity['session']).encode() in encoded
    bad=identity.copy();bad['session']=float(identity['session'])
    try:canonical_identity(bad)
    except AssertionError:pass
    else:raise AssertionError('rounded float identity accepted')
    results.append({'case':'identity-preserves-uint64-and-rejects-float','accepted':True})
    output={'status':'synthetic_reference_predicates_exercised_only','checks':len(results),'cases':results,
            'implementation_harness005_reviewed':False,'runtime_tests':0,'http_requests':0,
            'candidate_approved':False,'product_accepted':False}
    with (J/'pure-check.json').open('x',encoding='utf-8') as f:json.dump(output,f,indent=2);f.write('\n')
    print(json.dumps({k:v for k,v in output.items() if k!='cases'}))
