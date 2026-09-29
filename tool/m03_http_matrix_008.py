"""Four harness-only fixtures. --check is pure: no socket bind or host/plugin launch.
Runtime requires the coordinator's separate, explicit whole-group authorization.
Each case has one new operation; there is no retry, including after Unknown.
"""
import datetime
import copy
import hashlib
import http.server
import json
import os
import pathlib
import queue
import struct
import subprocess
import sys
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = ROOT / 'reports/codex-morrow-v1.1/host/m03-stream-001'
PLUGIN = pathlib.Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
HOST_MANIFEST = BASE / 'native-host-candidate-004/manifest.json'
PLUGIN_MANIFEST = PLUGIN / 'receipts/m03-stream-003/native-candidate-003.json'
HOST_MANIFEST_SHA = 'a1c46e448450f1066177e8f51f655f202757517f1cdb83451e7545ac68bbf6f2'
PLUGIN_MANIFEST_SHA = '54f4c6cd38e1968d1dbc54d26b8bf40460ee660498f8590c7f44c10fe04396c5'
HOST_SHA = 'c9c043c3104573146da1fe2a12a1f009a0a9b71ceb517f8bb6fffca1a4045224'
PLUGIN_SHA = 'bcfde17f4237bf72c1c55991c42a2656349ea7586527cd7557895fc12dc4c365'
SCHEMA_SHA = '8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864'
CASES = ('s503', 'r307', 'exact', 'over')
CAP = 65536


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, value):
    with path.open('x', encoding='utf-8') as out:
        json.dump(value, out, indent=2)
        out.write('\n')


def stamp():
    return datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')


def sse(value):
    return ('data: ' + json.dumps(value, separators=(',', ':')) + '\n\n').encode()


def fixture(case):
    if case in ('s503', 'r307'):
        status = 503 if case == 's503' else 307
        body = json.dumps({'error': {'message': f'synthetic {status}', 'type': 'server_error'}}, separators=(',', ':')).encode()
        return status, 'application/json', body
    prefix = sse({'type': 'response.created', 'response': {'id': 'resp_matrix'}})
    prefix += sse({'type': 'response.output_text.delta', 'delta': 'bounded matrix'})
    prefix += sse({'type': 'response.completed', 'response': {'id': 'resp_matrix', 'usage': {'input_tokens': 1, 'output_tokens': 1, 'total_tokens': 2}}})
    # One valid SSE comment after Completed; exact decoded HTTP body size, no framing bytes.
    body = prefix + b':' + b'x' * (CAP - len(prefix) - 3) + b'\n\n'
    assert len(body) == CAP
    return 200, 'text/event-stream', body + (b'x' if case == 'over' else b'')


def verify_candidates():
    assert sha(HOST_MANIFEST) == HOST_MANIFEST_SHA
    assert sha(PLUGIN_MANIFEST) == PLUGIN_MANIFEST_SHA
    hm = json.loads(HOST_MANIFEST.read_text())
    pm = json.loads(PLUGIN_MANIFEST.read_text())
    host, guest = pathlib.Path(hm['executable']), pathlib.Path(pm['executable'])
    assert sha(host) == hm['executable_sha256'] == HOST_SHA
    assert sha(guest) == pm['executable_sha256'] == PLUGIN_SHA
    return host, guest


def exact_prefix_ready(snapshot):
    p = snapshot['progress']
    assert not p['http_eof'], 'EOF observed before fixture release'
    assert p['intent'] != 'Observed' and not p['response_material_stored'], 'premature Observed'
    assert not p['request_closed'], 'premature RequestClosed'
    return all(p[k] == CAP for k in ('received_offset', 'reserved_offset', 'issued_offset', 'os_completed_offset', 'peer_consumed_offset'))


def request_digest(identity, operation, proposal):
    body = bytes.fromhex(proposal['body_hex'])
    assert hashlib.sha256(body).hexdigest() == proposal['body_sha256']
    u32, u64 = lambda n: struct.pack('<I', n), lambda n: struct.pack('<Q', n)
    field = lambda b: u32(len(b)) + b
    pre = b'Morrow/native-http-proposal/v1\0' + u64(identity['session']) + u64(identity['epoch'])
    pre += field(operation.encode()) + u64(1) + field(proposal['method'].encode()) + field(proposal['absolute_target'].encode()) + u32(len(proposal['headers']))
    for h in proposal['headers']:
        pre += field(h['name'].encode()) + field(bytes.fromhex(h['value_hex']))
    pre += u32(proposal['response_limit']) + field(body)
    digest = hashlib.sha256(pre).hexdigest()
    assert digest == proposal['request_sha256']
    return body, digest


def control_checks(case, final, guest, operation):
    """Independent control evidence is mandatory even for expected aggregate Unknown."""
    s = final['snapshot']; p = s['http']['progress']
    c = guest['close_observation']; ack = c['matching_ack']; identity = guest['identity']
    close_writes = [f for f in guest['frames'] if f['kind'] == 'Close' and f['lane'] == 'guest_control_written']
    host_acks = [e['detail'] for e in s['events'] if e['event'] == 'close_ack_written']
    spawn = next(e['detail'] for e in s['events'] if e['event'] == 'spawn')
    checks = {
        'close_written_and_acked': c['close_written'] is True and c['close_acked'] is True,
        'control_boundary_eof_observed': c['control_eof'] is True,
        'control_router_end_observed': c['control_stream_ended'] is True,
        'independent_control_failure_absent': c['sticky_control_failure'] is None,
        'control_protocol_clean': c['control_protocol_clean'] is True,
        'control_end_result_ok_separately': guest['control_end_result'] == {'Ok': None},
        'matching_ack_identity': isinstance(ack, dict) and all(ack[k] == value for k, value in {'session': s['session'], 'epoch': s['epoch'], 'child_pid': s['pid'], 'attempt': 1, 'generation': s['generation'], 'code': 0}.items()),
        'matching_ack_sequence': isinstance(ack, dict) and len(close_writes) == 1 and len(host_acks) == 1 and ack['sequence'] == close_writes[0]['sequence'] == host_acks[0]['sequence'],
        'guest_identity_matches_batch': all(identity[k] == value for k, value in {'session': s['session'], 'epoch': s['epoch'], 'child_pid': s['pid'], 'attempt': 1, 'schema_sha256': SCHEMA_SHA, 'artifact_sha256': PLUGIN_SHA, 'host_execution_config_sha256': spawn['config_sha256'], 'operation_id_sha256': hashlib.sha256(operation.encode()).hexdigest()}.items()),
    }
    # Owner release/child exit are deliberately excluded: guest cannot observe its own exit.
    fields = ('intent', 'network', 'http_status', 'error_code', 'received_offset', 'reserved_offset', 'issued_offset', 'os_completed_offset', 'peer_consumed_offset', 'parser_yielded_bytes', 'drain_discarded_bytes', 'cancel_discarded_bytes', 'error_consumed_bytes', 'http_eof', 'response_material_stored', 'worker_started', 'worker_joined', 'connect_reaped', 'read_reaped', 'write_reaped', 'data_closed', 'request_closed', 'revoke_applied', 'revoke_persisted')
    gp = c['final_progress']
    checks['host_guest_terminal_progress_agrees'] = isinstance(gp, dict) and all(gp[k] == p[k] for k in fields)
    cleanup = guest['task']['cleanup'].get('Ok')
    mapping = {'received_offset': 'received_offset', 'delivered_offset': 'peer_consumed_offset', 'acknowledged_offset': 'peer_consumed_offset', 'durable_observed': 'response_material_stored', 'network_eof': 'http_eof', 'request_closed': 'request_closed', 'network_worker_started': 'worker_started', 'network_worker_exited': 'worker_joined', 'data_channel_closed': 'data_closed', 'data_connect_reaped': 'connect_reaped', 'data_read_reaped': 'read_reaped', 'data_write_reaped': 'write_reaped'}
    checks['host_guest_cleanup_agrees'] = isinstance(cleanup, dict) and all(cleanup[g] == p[h] for g, h in mapping.items())
    if case == 'exact':
        checks['aggregate_clean_for_positive'] = c['aggregate_error'] is None and guest['close_result'] == {'Ok': None} and guest['final_control_result'] == {'Ok': None}
    else:
        checks['expected_aggregate_error_retained'] = c['aggregate_error'] == 'Unknown' and guest['close_result'] == {'Err': 'Unknown'} and guest['final_control_result'] == {'Err': 'Unknown'}
        checks['expected_native_error_reason'] = p['error_code'] == (21 if case == 'over' else 19)
    return checks


def validate_result(case, final, guest, host_rc, server_rows, sink_rows, gate_snapshot, operation):
    """Keep separate dimensions; expected Core failure is not HTTP/material failure."""
    p, s = final['snapshot']['http']['progress'], final['snapshot']
    checks = {}
    def check(name, value):
        checks[name] = bool(value)
    check('exactly_one_post', sum(r['event'] == 'post_received' for r in server_rows) == 1)
    check('no_redirect_sink_hits', not sink_rows)
    check('server_fixture_no_error', not any(r['event'] == 'server_error' for r in server_rows))
    check('host_control_exit_0', host_rc == 0)
    check('guest_expected_exit', s['exit_code'] == (0 if case == 'exact' else 2))
    check('real_child_exit_and_dual_eof', s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'])
    check('durable_owner_released', final['state']['owner']['phase'] == 'Released' and not final['state']['owner']['owner_retained'])
    check('request_and_native_workers_closed', p['request_closed'] and p['worker_started'] and p['worker_joined'] and p['data_closed'] and p['connect_reaped'] and p['read_reaped'] and p['write_reaped'])
    check('guest_data_worker_joined', guest['local_data_thread_joined'])
    checks.update(control_checks(case, final, guest, operation))
    check('no_evidence_overflow', not guest['frames_overflow'] and not guest['io_overflow'] and not guest['task']['audit']['overflow'] and not s['event_overflow'])
    terminal = guest['task']['core_terminal']
    status, _, body = fixture(case)
    check('http_status_preserved', p['http_status'] == status)
    check('classification_sum', p['peer_consumed_offset'] == sum(p[k] for k in ('parser_yielded_bytes', 'drain_discarded_bytes', 'error_consumed_bytes', 'cancel_discarded_bytes')))
    if case != 'over':
        check('complete_http_observed_separately', p['intent'] == 'Observed' and p['http_eof'] and p['response_material_stored'])
        check('full_response_offsets', all(p[k] == len(body) for k in ('received_offset', 'reserved_offset', 'issued_offset', 'os_completed_offset', 'peer_consumed_offset')))
        check('server_eof_sent', any(r['event'] == 'http_eof_sent' for r in server_rows))
    if case in ('s503', 'r307'):
        check('core_reports_error_not_completed', isinstance(terminal, dict) and 'Failed' in terminal)
        check('error_body_classified', p['error_consumed_bytes'] == len(body) and p['parser_yielded_bytes'] == 0)
    else:
        check('actual_pre_eof_snapshot', gate_snapshot is not None and exact_prefix_ready(gate_snapshot['snapshot']))
        check('core_completed_before_tail_outcome', isinstance(terminal, dict) and 'Completed' in terminal)
        if case == 'exact':
            check('transport_drain_ok', guest['task']['transport_drain'] == {'Ok': None})
            check('final_control_ok', guest['final_control_result'] == {'Ok': None})
            check('no_http_error', p['error_code'] == 0)
        else:
            check('over_cap_unknown_not_observed', p['intent'] == 'Unknown' and not p['http_eof'] and not p['response_material_stored'])
            # Require both the original quota event and the retained final reason21.
            check('quota_error_observed', any(e['event'] == 'http_cancel_applied' and e['detail']['code'] == 21 for e in s['events']))
            check('cap_offsets_preserved_no_extra_delivery', all(p[k] == CAP for k in ('received_offset', 'reserved_offset', 'issued_offset', 'os_completed_offset', 'peer_consumed_offset')))
            check('extra_byte_written_by_server', any(r['event'] == 'over_cap_byte_sent' for r in server_rows))
            check('transport_drain_failed', isinstance(guest['task']['transport_drain'], dict) and 'Err' in guest['task']['transport_drain'])
    return checks


def run_case(batch, batch_stamp, case):
    run = batch / case
    run.mkdir()
    host, guest = verify_candidates()
    evidence = PLUGIN / 'out/m03-host-matrix-008' / batch_stamp / case
    evidence.mkdir(parents=True)
    profile, cwd = run / 'profile', run / 'cwd'
    profile.mkdir(); cwd.mkdir()
    status, content_type, response = fixture(case)
    (run / 'response-body.bin').write_bytes(response)
    dump(run / 'inputs.json', {'host': str(host), 'host_sha256': HOST_SHA, 'host_manifest_sha256': HOST_MANIFEST_SHA, 'guest': str(guest), 'guest_sha256': PLUGIN_SHA, 'plugin_manifest_sha256': PLUGIN_MANIFEST_SHA, 'script_sha256': sha(pathlib.Path(__file__)), 'response_sha256': hashlib.sha256(response).hexdigest(), 'response_bytes': len(response), 'handshake_ms': 6000, 'original_ttl_ms': 10000, 'automatic_retry': False})
    started = time.monotonic_ns()
    server_rows, sink_rows, rows, commands = [], [], [], []
    prefix_written, release_tail, abort = threading.Event(), threading.Event(), threading.Event()
    approved_body = None
    def mark(target, event, **kw):
        target.append({'at_ns': time.monotonic_ns() - started, 'event': event, **kw})
    class Sink(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def hit(self):
            mark(sink_rows, 'unexpected_redirect', method=self.command, path=self.path)
            self.send_response(204); self.send_header('Content-Length', '0'); self.end_headers()
            self.close_connection = True
        do_GET = do_POST = do_HEAD = do_PUT = do_DELETE = do_PATCH = hit
    sink = None
    if case == 'r307':
        sink = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Sink)
        sink.daemon_threads = True
        threading.Thread(target=sink.serve_forever, daemon=True).start()
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = 'HTTP/1.1'
        def log_message(self, *args): pass
        def do_POST(self):
            try:
                self.connection.settimeout(8)
                n = int(self.headers['Content-Length']); assert 0 <= n <= 32768
                body = self.rfile.read(n)
                mark(server_rows, 'post_received', path=self.path, headers=list(self.headers.items()), body_hex=body.hex())
                assert sum(r['event'] == 'post_received' for r in server_rows) == 1, 'unexpected repeated POST'
                assert body == approved_body and self.path == '/v1/responses'
                assert 'Authorization' not in self.headers
                self.send_response(status); self.send_header('Content-Type', content_type)
                self.send_header('Transfer-Encoding', 'chunked')
                if sink is not None:
                    self.send_header('Location', f'http://127.0.0.1:{sink.server_address[1]}/sink')
                self.end_headers()
                def chunk(data):
                    self.wfile.write(f'{len(data):x}\r\n'.encode() + data + b'\r\n'); self.wfile.flush()
                if case in ('exact', 'over'):
                    chunk(response[:CAP]); mark(server_rows, 'cap_prefix_sent', body_bytes=CAP)
                    prefix_written.set()
                    until = time.monotonic() + 7
                    while not release_tail.wait(.005):
                        if abort.is_set() or time.monotonic() >= until:
                            raise RuntimeError('no EOF/extra-byte release without qualified host snapshot')
                    if abort.is_set(): raise RuntimeError('fixture aborted before tail release')
                    mark(server_rows, 'tail_gate_opened_after_host_snapshot')
                    if case == 'over':
                        chunk(response[CAP:]); mark(server_rows, 'over_cap_byte_sent', bytes=1)
                else:
                    chunk(response)
                self.wfile.write(b'0\r\n\r\n'); self.wfile.flush(); mark(server_rows, 'http_eof_sent')
            except Exception as exc:
                mark(server_rows, 'server_error', error=repr(exc)); self.close_connection = True
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    origin = f'http://127.0.0.1:{server.server_address[1]}'
    env = {k: os.environ[k] for k in ('SYSTEMROOT', 'WINDIR', 'COMSPEC', 'LOCALAPPDATA') if k in os.environ}
    proc = errfile = reader_thread = final = gate_snapshot = identity = None
    result = {'case': case, 'status': 'failed', 'plugin_evidence': str(evidence), 'terminal_poll': 'not directly instrumented; source/state inference only', 'checks': {}}
    def retained_final():
        return next((r['value'] for r in reversed(rows) if r['value'].get('event') == 'final'), None)
    def stopped_error():
        f = retained_final()
        reasons = [] if f is None else [e for e in f['snapshot']['events'] if e['event'] in ('session_close_reason', 'request_denied', 'network_supervision_error', 'pipe_driver_error')]
        return RuntimeError('host ended before expected result: ' + json.dumps(reasons))
    try:
        init = subprocess.run([str(host), 'init', '--profile', str(profile), '--slot', 'matrix-slot'], cwd=ROOT, env=env, capture_output=True, timeout=10)
        dump(run / 'init.json', {'exit': init.returncode, 'stdout': init.stdout.decode(), 'stderr': init.stderr.decode()})
        assert init.returncode == 0
        operation = 'm03-' + batch_stamp + '-' + case
        args = [str(host), 'serve', '--profile', str(profile), '--client', str(guest), '--sha256', PLUGIN_SHA, '--work-dir', str(cwd), '--plugin-id', 'morrow-codex', '--role', 'synthetic-http', '--operation', operation, '--http-origin', origin, '--ttl-ms', '10000', '--handshake-ms', '6000', '--close-ms', '1000']
        for value in ('--fixture-base', origin + '/v1', '--evidence-dir', str(evidence), '--max-chunk', '1024'):
            args += ['--client-arg', value]
        dump(run / 'launch.json', {'args': args, 'env': env})
        messages = queue.Queue(); errfile = (run / 'host.stderr').open('wb')
        proc = subprocess.Popen(args, cwd=ROOT, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errfile, text=True, encoding='utf-8')
        def reader():
            try:
                for line in proc.stdout:
                    value = json.loads(line); rows.append({'at_ns': time.monotonic_ns() - started, 'value': value}); messages.put(value)
            finally:
                messages.put(None)
        reader_thread = threading.Thread(target=reader, daemon=True); reader_thread.start()
        def receive(predicate, timeout=12):
            until = time.monotonic() + timeout
            while time.monotonic() < until:
                value = messages.get(timeout=max(.01, until - time.monotonic()))
                if value is None: raise stopped_error()
                if predicate(value): return value
                if value.get('event') == 'final': raise stopped_error()
            raise TimeoutError('host result timeout')
        def command(value):
            if retained_final() is not None or proc.poll() is not None: raise stopped_error()
            mark(commands, 'operator_command', value=value)
            try:
                proc.stdin.write(json.dumps(value) + '\n'); proc.stdin.flush()
            except OSError:
                reader_thread.join(timeout=.2)
                if retained_final() is not None or proc.poll() is not None: raise stopped_error()
                raise
            reply = receive(lambda r: r.get('event') == 'operator_result' and r.get('action') == value['action'])
            assert reply['ok'], reply
            return reply['result']
        receive(lambda r: r.get('event') == 'proposal')
        grant = command({'action': 'approve'})['grant_id']
        identity = command({'action': 'claim', 'grant_id': grant})
        until = time.monotonic() + 6
        while time.monotonic() < until:
            proposal = command({'action': 'inspect_http'}).get('proposal')
            if proposal: break
            time.sleep(.015)
        else: raise RuntimeError('complete HTTP proposal absent')
        body, digest = request_digest(identity, operation, proposal)
        assert proposal['method'] == 'POST' and proposal['absolute_target'] == origin + '/v1/responses'
        assert proposal['response_limit'] == CAP
        parsed = json.loads(body); assert parsed.get('stream') is True and not parsed.get('tools')
        assert not server_rows and not sink_rows
        approved_body = body
        dump(run / 'independent-approval.json', {'proposal': proposal, 'operation': operation, 'identity': identity, 'recomputed_request_sha256': digest})
        command({'action': 'approve_http', 'proposal_ref': proposal['proposal_ref'], 'expected_hash': digest, 'response_limit': CAP})
        if case in ('exact', 'over'):
            until = time.monotonic() + 5
            snapshots = []
            while time.monotonic() < until:
                snap = command({'action': 'inspect_http'})
                observation = {'at_ns': time.monotonic_ns() - started, 'snapshot': snap}
                snapshots.append(observation)
                if prefix_written.is_set() and exact_prefix_ready(snap):
                    gate_snapshot = observation
                    # Write actual inspected facts before permitting EOF or excess byte.
                    dump(run / 'pre-eof-host-snapshot.json', gate_snapshot)
                    dump(run / 'pre-eof-inspections.json', snapshots)
                    release_tail.set()
                    break
                time.sleep(.01)
            else:
                dump(run / 'pre-eof-inspections.json', snapshots)
                raise RuntimeError('full acknowledged cap prefix not reached; EOF withheld')
        final = receive(lambda r: r.get('event') == 'final')
        host_rc = proc.wait(timeout=5); reader_thread.join(timeout=1)
        guest_result = json.loads((evidence / 'result.json').read_text())
        result['dimensions'] = {'core_terminal': guest_result['task']['core_terminal'], 'transport_drain': guest_result['task']['transport_drain'], 'guest_cleanup': guest_result['task']['cleanup'], 'guest_close': guest_result['close_result'], 'guest_final_control': guest_result['final_control_result'], 'close_observation': guest_result['close_observation'], 'control_end_result': guest_result['control_end_result'], 'http_and_native': final['snapshot']['http']['progress'], 'host_exit': host_rc, 'guest_exit': final['snapshot']['exit_code']}
        result['checks'] = validate_result(case, final, guest_result, host_rc, server_rows, sink_rows, gate_snapshot, operation)
        result['status'] = 'passed' if all(result['checks'].values()) else 'failed_expectations'
    except Exception as exc:
        result['error'] = repr(exc)
    finally:
        abort.set()
        if proc is not None and proc.poll() is None:
            try:
                proc.stdin.write('{"action":"stop"}\n'); proc.stdin.flush(); proc.wait(timeout=15)
            except Exception as exc: result['cleanup_error'] = repr(exc)
        if proc is not None:
            try: proc.stdin.close()
            except OSError as exc: result['stdin_close_error'] = repr(exc)
        if errfile is not None: errfile.close()
        if reader_thread is not None and proc.poll() is not None: reader_thread.join(timeout=1)
        server.shutdown(); server.server_close()
        if sink is not None: sink.shutdown(); sink.server_close()
        final = final or retained_final()
        dump(run / 'host-rows.json', rows); dump(run / 'operator-commands.json', commands)
        dump(run / 'server-events.json', server_rows); dump(run / 'sink-events.json', sink_rows)
        if final is not None: dump(run / 'retained-final.json', final)
        result['host_lifetime'] = {'pid': None if proc is None else proc.pid, 'exit': None if proc is None else proc.poll(), 'status': 'not_started' if proc is None else ('running_cleanup_unconfirmed' if proc.poll() is None else 'os_exit_observed'), 'child_identity': identity, 'owner_release_verified': bool(final and final['state']['owner']['owner_retained'] is False)}
        result['post_count'] = sum(r['event'] == 'post_received' for r in server_rows)
        result['sink_count'] = len(sink_rows)
        result['fixed_artifacts_unchanged'] = sha(host) == HOST_SHA and sha(guest) == PLUGIN_SHA and sha(HOST_MANIFEST) == HOST_MANIFEST_SHA and sha(PLUGIN_MANIFEST) == PLUGIN_MANIFEST_SHA
        dump(run / 'result.json', result)
        dump(run / 'evidence-manifest.json', {'files': {str(p.relative_to(run)).replace('\\', '/'): sha(p) for p in sorted(run.rglob('*')) if p.is_file()}, 'plugin_evidence': {str(p): sha(p) for p in sorted(evidence.rglob('*')) if p.is_file()}})
    return result


def pure_control_checks():
    """Synthetic dictionaries only: exercises verdicts, never claims runtime proof."""
    operation = 'pure-fixture-no-operation-launched'
    ack = dict(session=123, epoch=1, child_pid=456, attempt=1, sequence=8, generation=2, code=0)
    p = dict(intent='Observed', network='Eof', http_status=503, error_code=19,
             received_offset=59, reserved_offset=59, issued_offset=59,
             os_completed_offset=59, peer_consumed_offset=59,
             parser_yielded_bytes=0, drain_discarded_bytes=0,
             cancel_discarded_bytes=0, error_consumed_bytes=59)
    p.update({k: True for k in ('http_eof', 'response_material_stored', 'worker_started',
        'worker_joined', 'connect_reaped', 'read_reaped', 'write_reaped', 'data_closed',
        'request_closed', 'revoke_applied', 'revoke_persisted')})
    final = {'snapshot': {'session': 123, 'epoch': 1, 'pid': 456, 'generation': 2,
        'http': {'progress': p}, 'events': [
            {'event': 'spawn', 'detail': {'config_sha256': 'a' * 64}},
            {'event': 'close_ack_written', 'detail': {'sequence': 8}}]}}
    cleanup = dict(received_offset=59, delivered_offset=59, acknowledged_offset=59)
    cleanup.update({k: True for k in ('durable_observed', 'network_eof', 'request_closed',
        'network_worker_started', 'network_worker_exited', 'data_channel_closed',
        'data_connect_reaped', 'data_read_reaped', 'data_write_reaped')})
    guest = {'identity': dict(session=123, epoch=1, child_pid=456, attempt=1,
        artifact_sha256=PLUGIN_SHA, schema_sha256=SCHEMA_SHA,
        host_execution_config_sha256='a' * 64,
        operation_id_sha256=hashlib.sha256(operation.encode()).hexdigest()),
        'frames': [{'kind': 'Close', 'lane': 'guest_control_written', 'sequence': 8}],
        'task': {'cleanup': {'Ok': cleanup}},
        'close_result': {'Err': 'Unknown'}, 'final_control_result': {'Err': 'Unknown'},
        'control_end_result': {'Ok': None}, 'close_observation': {
            'close_written': True, 'close_acked': True, 'matching_ack': ack,
            'aggregate_error': 'Unknown', 'sticky_control_failure': None,
            'control_eof': True, 'control_stream_ended': True,
            'control_protocol_clean': True, 'final_progress': copy.deepcopy(p)}}
    checks = control_checks('s503', final, guest, operation)
    assert all(checks.values()), checks
    rejected = []
    def reject(label, path, value):
        changed = copy.deepcopy(guest)
        parent = changed
        for key in path[:-1]: parent = parent[key]
        parent[path[-1]] = value
        try:
            verdicts = control_checks('s503', final, changed, operation)
        except (KeyError, TypeError):
            rejected.append(label + ': missing/malformed evidence fails closed')
            return
        assert not all(verdicts.values()), label
        rejected.append(label)
    for key in ('close_written', 'close_acked', 'control_eof', 'control_stream_ended', 'control_protocol_clean'):
        reject(key + '_false', ('close_observation', key), False)
        reject(key + '_null', ('close_observation', key), None)
    reject('Unknown_cannot_hide_later_protocol_failure',
        ('close_observation', 'sticky_control_failure'), {'source': 'sequence', 'error': 'Protocol'})
    reject('end_Ok_and_ACK_cannot_hide_missing_EOF', ('close_observation', 'control_eof'), False)
    reject('end_error', ('control_end_result',), {'Err': 'Protocol'})
    reject('missing_ACK', ('close_observation', 'matching_ack'), None)
    for key in ack:
        reject('ACK_wrong_' + key, ('close_observation', 'matching_ack', key), ack[key] + 1)
    reject('different_operation', ('identity', 'operation_id_sha256'), 'b' * 64)
    reject('different_schema', ('identity', 'schema_sha256'), 'b' * 64)
    reject('wrong_terminal_offset', ('close_observation', 'final_progress', 'received_offset'), 58)
    reject('wrong_cleanup_offset', ('task', 'cleanup', 'Ok', 'acknowledged_offset'), 58)
    reject('aggregate_error_erased', ('close_observation', 'aggregate_error'), None)
    reject('aggregate_protocol_error_not_expected_Unknown', ('close_observation', 'aggregate_error'), 'Protocol')
    positive_guest = copy.deepcopy(guest)
    positive_guest['close_observation']['aggregate_error'] = None
    positive_guest['close_result'] = positive_guest['final_control_result'] = {'Ok': None}
    positive = control_checks('exact', final, positive_guest, operation)
    assert all(positive.values()), positive
    # Only the control classifier is exercised here; these are not HTTP case results.
    return {'synthetic_only': True, 'accepted_control_shapes': ['expected aggregate Unknown with independently clean control', 'aggregate clean with independently clean control'], 'rejected_mutations': rejected}


def pure_check():
    facts = []
    for case in CASES:
        status, content_type, body = fixture(case)
        assert status == {'s503': 503, 'r307': 307, 'exact': 200, 'over': 200}[case]
        if case in ('exact', 'over'):
            assert len(body) == CAP + (case == 'over')
            assert body[:CAP].endswith(b'\n\n')
            assert b'"type":"response.completed"' in body[:CAP]
        else:
            assert json.loads(body)['error']['message'] == f'synthetic {status}'
        facts.append({'case': case, 'status': status, 'content_type': content_type, 'body_bytes': len(body), 'body_sha256': hashlib.sha256(body).hexdigest()})
    assert fixture('over')[2] == fixture('exact')[2] + b'x'
    p = {k: CAP for k in ('received_offset', 'reserved_offset', 'issued_offset', 'os_completed_offset', 'peer_consumed_offset')}
    p.update(http_eof=False, intent='Unknown', response_material_stored=False, request_closed=False)
    assert exact_prefix_ready({'progress': p})
    for key, value in [('http_eof', True), ('intent', 'Observed'), ('response_material_stored', True), ('request_closed', True)]:
        changed = dict(p); changed[key] = value
        try: exact_prefix_ready({'progress': changed})
        except AssertionError: pass
        else: raise AssertionError('gate accepted premature terminal facts')
    changed = dict(p); changed['peer_consumed_offset'] -= 1
    assert not exact_prefix_ready({'progress': changed})
    control = pure_control_checks()
    host, guest = verify_candidates()  # Read/hash only, never execute these files.
    report = {'status': 'syntax_and_pure_fixtures_checked_runtime_not_run', 'cases': facts, 'control_classifier': control, 'fixed_host': str(host), 'fixed_plugin': str(guest), 'host_sha256': HOST_SHA, 'plugin_sha256': PLUGIN_SHA, 'host_manifest_sha256': HOST_MANIFEST_SHA, 'plugin_manifest_sha256': PLUGIN_MANIFEST_SHA, 'script_sha256': sha(pathlib.Path(__file__)), 'socket_bound': False, 'subprocess_started': False, 'post_count': 0, 'gate_checks': 'rejects EOF/Observed/material/RequestClosed and waits for all five exact-cap offsets', 'terminal_poll_claim': 'no direct observation instrumentation; runtime will report source/state inference only'}
    path = BASE / ('matrix-008-pure-check-' + stamp() + '.json')
    dump(path, report)
    print(json.dumps({'path': str(path), **report}))


def main():
    if sys.argv[1:] == ['--check']:
        pure_check(); return 0
    if sys.argv[1:] != ['--execute-authorized-four-case-group']:
        raise SystemExit('Use --check only until coordinator authorizes the whole four-case run.')
    batch_stamp = stamp(); batch = BASE / ('http-matrix-008-' + batch_stamp); batch.mkdir()
    dump(batch / 'locked-expectations.json', {
        'host_sha256': HOST_SHA, 'guest_sha256': PLUGIN_SHA, 'script_sha256': sha(pathlib.Path(__file__)),
        'case_order': list(CASES), 'handshake_ms': 6000, 'original_ttl_ms': 10000,
        'all_cases': ['one original POST', 'no redirect sink hit', 'host exit0', 'actual child exit and dual EOF', 'durable ownerReleased', 'RequestClosed and both worker joins', 'same-lock Close written/acked with exact batch identity/sequence/generation/code', 'control EOF and ended with clean verdict and independent failure null', 'control_end Ok alone insufficient', 'guest/host final progress and cleanup agreement', 'no evidence overflow'],
        's503_r307': ['Core Failed, guest exit2', 'complete response retains Observed/material/EOF', 'all response offsets match error body', 'error body classification without parser output', 'aggregate Unknown and both existing close/finalcontrol ErrUnknown expected, independently clean control required', 'host reason19 from deliberate cancellation retained'],
        'exact': ['actual inspected cap offsets before EOF release', 'pre-EOF not Observed/material/RequestClosed', 'Core Completed, drain Ok, finalcontrol Ok, guest exit0', 'after real EOF Observed with error0'],
        'over': ['same exact cap gate then one extra byte', 'preserve earlier Core Completed, drain Err, guest exit2', 'Unknown with quota observation; no HTTP EOF/material/Observed', 'extra byte not delivered', 'aggregate Unknown and old close/finalcontrol ErrUnknown retained, clean independent control required', 'host final quota reason21 retained'],
        'terminal_poll_observation': 'not instrumented; source/state inference only', 'automatic_retry': False,
        'on_unexpected_failure': 'preserve evidence and stop remaining cases'})
    results = []
    for case in CASES:
        result = run_case(batch, batch_stamp, case); results.append(result)
        # Never continue through an unresolved/failed expectation or repeat its POST.
        if result['status'] != 'passed' or not result['fixed_artifacts_unchanged']: break
    summary = {'status': 'passed' if len(results) == 4 and all(r['status'] == 'passed' for r in results) else 'stopped_on_failure', 'results': results, 'automatic_retry': False, 'full_m03_qualified': False}
    dump(batch / 'result.json', summary); print(json.dumps({'batch': str(batch), **summary}))
    return 0 if summary['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
