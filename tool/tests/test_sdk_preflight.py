"""Consumer protocol tests; synthetic responses are not real host qualification."""
import contextlib
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import morrow_plugin as tool
import sdk_profiles as sdk
from test_sdk_profiles import descriptor, detailed_descriptor


def advertised():
    value = detailed_descriptor('linux')
    value['profiles'][0]['hard_byte_limits']['module'] = sdk.MAX_MODULE_BYTES
    value['diagnostic_capabilities'] = {'package_preflight': {
        'schema_version': 1, 'command': '--sdk-preflight', 'read_only': True,
        'preparation': 'static_only', 'max_output_bytes': 16384, 'authority': 'none',
        'hard_byte_limits': {'archive': 4276930, 'module': 4194304}}}
    return value


def response(archive, discovered=None):
    discovered = discovered or advertised()
    limits = {'fuel': 100000, 'memory_bytes': 65536, 'host_calls': 0}
    return {'schema_version': 1, 'diagnostic': 'package_preflight',
            'host_version': discovered['host_version'], 'platform': discovered['platform'],
            'backend': discovered['backend'], 'status': 'prepared', 'phase': 'static_preparation',
            'preparation': 'static_only', 'grants_created': 0,
            'authority': 'none', 'guest_executed': False, 'installed': False,
            'production_qualified': False, 'routes_qualified': False,
            'host_limits': discovered['profiles'][0]['runtime_defaults'],
            'effective_limits': limits.copy(), 'hard_byte_limits': sdk.PREFLIGHT_BYTE_LIMITS.copy(),
            'error': None, 'package': {'id': 'example.preflight', 'version': '0.1.0',
                'archive_sha256': hashlib.sha256(archive).hexdigest(), 'module_sha256': 'a'*64,
                'archive_bytes': len(archive), 'module_bytes': 8, 'manifest_schema_version': 1,
                'guest_abi_version': 2, 'declared_limits': limits.copy()}}


def encoded(value):
    return json.dumps(value).encode('utf-8')


def rejection(archive, code='package_rejected'):
    value = response(archive)
    phases = {'invalid_arguments': 'arguments', 'invalid_file_type': 'package_read_decode',
              'package_unavailable': 'package_read_decode', 'package_rejected': 'package_read_decode',
              'preparation_rejected': 'static_preparation', 'output_limit': 'static_preparation'}
    value.update(status='rejected', phase=phases[code], effective_limits=None,
                 error={'code': code, 'message': 'Bounded rejection'})
    if code != 'preparation_rejected': value['package'] = None
    return value


class PackagePreflight(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.host = self.root/'trusted-host'
        self.host.write_bytes(b'synthetic-host-fixture')
        self.archive = b'synthetic-selected-archive'
        self.package = self.root/'selected.mplugin'
        self.package.write_bytes(self.archive)

    def call(self, discovery=None, result=None, exit_code=0):
        with mock.patch.object(sdk, 'bounded_process_result', side_effect=[
                (0, encoded(advertised() if discovery is None else discovery)),
                (exit_code, encoded(response(self.archive) if result is None else result))]) as run:
            output = sdk.preflight(self.host, self.package)
        return output, run.call_args_list

    def test_optional_advertisement_preserves_old_profiles(self):
        for value in (descriptor(), detailed_descriptor('linux'), advertised()):
            self.assertIs(sdk.validate_descriptor(value), value)
        projected = advertised()
        del projected['diagnostic_capabilities']
        old = detailed_descriptor('linux')
        old['profiles'][0]['hard_byte_limits']['module'] = sdk.MAX_MODULE_BYTES
        self.assertEqual(projected, old)

    def test_known_capability_is_required_before_new_command_or_archive_read(self):
        for old in (descriptor(), detailed_descriptor('linux')):
            with mock.patch.object(sdk, 'bounded_process', return_value=encoded(old)) as run:
                with self.assertRaisesRegex(ValueError, 'does not advertise'):
                    sdk.preflight(self.host, self.root/'missing.mplugin')
                self.assertEqual(run.call_args_list, [mock.call([str(self.host), '--sdk-capabilities'])])

    def test_capability_unknown_missing_malformed_and_contradictory_claims_fail_before_command(self):
        values = []
        for field, bad in [('schema_version', True), ('schema_version', 2),
                           ('read_only', 1), ('read_only', False), ('preparation', 'execute'),
                           ('authority', 'granted'), ('command', '--install'),
                           ('command', ['--sdk-preflight']), ('max_output_bytes', 16384.0),
                           ('max_output_bytes', 65536), ('hard_byte_limits', {'archive': 1 << 63, 'module': 4194304}),
                           ('hard_byte_limits', {'archive': 4276930.0, 'module': 4194304})]:
            item = advertised(); item['diagnostic_capabilities']['package_preflight'][field] = bad
            values.append(item)
        for field in advertised()['diagnostic_capabilities']['package_preflight']:
            item = advertised(); del item['diagnostic_capabilities']['package_preflight'][field]
            values.append(item)
        for bad in (None, [], {}, {'unknown': {}}, {'package_preflight': None}):
            item = advertised(); item['diagnostic_capabilities'] = bad; values.append(item)
        item = advertised(); item['diagnostic_capabilities']['package_preflight']['installs'] = True; values.append(item)
        for value in values:
            with self.subTest(value=value['diagnostic_capabilities']), \
                    mock.patch.object(sdk, 'bounded_process', return_value=encoded(value)) as run:
                with self.assertRaises(ValueError): sdk.preflight(self.host, self.package)
                self.assertEqual(run.call_count, 1)

    def test_compiled_module_and_default_limit_contradictions_are_refused(self):
        for field, bad in [('module', 1), ('fuel', 100000001), ('host_calls', 1025), ('memory_bytes', 65537)]:
            item = advertised()
            item['profiles'][0]['hard_byte_limits' if field == 'module' else 'runtime_defaults'][field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError): sdk.validate_descriptor(item)
        item = advertised(); second = copy.deepcopy(item['profiles'][0]); second['id'] = 'other'
        second['runtime_defaults']['fuel'] -= 1; item['profiles'].append(second)
        with self.assertRaisesRegex(ValueError, 'inconsistent'): sdk.validate_descriptor(item)

    def test_success_binds_exact_selected_archive_host_descriptor_and_response(self):
        other = self.root/'newer.mplugin'; other.write_bytes(b'not-selected')
        result, calls = self.call()
        self.assertEqual(calls, [mock.call([str(self.host), '--sdk-capabilities'], max_output=65536),
                                mock.call([str(self.host), '--sdk-preflight', str(self.package)], max_output=16384)])
        self.assertEqual(result['host_sha256'], hashlib.sha256(self.host.read_bytes()).hexdigest())
        self.assertEqual(result['archive_sha256'], hashlib.sha256(self.archive).hexdigest())
        self.assertEqual(result['descriptor_sha256'], hashlib.sha256(encoded(advertised())).hexdigest())
        self.assertEqual(result['response_sha256'], hashlib.sha256(encoded(response(self.archive))).hexdigest())
        self.assertEqual(result['preflight']['effective_limits']['host_calls'], 0)
        self.assertEqual(other.read_bytes(), b'not-selected')

    def test_effective_limits_are_exact_minimum_in_each_dimension(self):
        item = response(self.archive)
        item['package']['declared_limits'] = {'fuel': 100000000, 'memory_bytes': 67108864, 'host_calls': 1024}
        item['effective_limits'] = item['host_limits'].copy()
        self.call(result=item)
        for field, delta in [('fuel', 1), ('memory_bytes', 65536), ('host_calls', 1)]:
            for sign in (-1, 1):
                bad = copy.deepcopy(item); bad['effective_limits'][field] += sign * delta
                with self.subTest(field=field, sign=sign), self.assertRaisesRegex(ValueError, 'intersection'):
                    self.call(result=bad)

    def test_host_identity_archive_binding_and_bounded_facts_must_agree(self):
        mutations = [('host_version', 'wrong'), ('platform', {'os': 'windows', 'arch': 'x86_64'}),
                     ('backend', 'other'), ('hard_byte_limits', {'archive': 4276930, 'module': 4194304.0})]
        for field, value in mutations:
            item = response(self.archive); item[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError): self.call(result=item)
        for field, value in [('archive_sha256', 'b'*64), ('archive_bytes', len(self.archive)+1),
                             ('archive_bytes', True), ('module_sha256', 'A'*64), ('module_bytes', 4194305),
                             ('module_bytes', 7), ('manifest_schema_version', True), ('guest_abi_version', 3),
                             ('id', 'unsafe/path'), ('id', 'bad\x7f'), ('id', 'bad\x85'), ('id', 'x'*257), ('version', 'x'*129)]:
            item = response(self.archive); item['package'][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError): self.call(result=item)
        item = response(self.archive); item['host_limits']['fuel'] = 1
        with self.assertRaisesRegex(ValueError, 'host limits'): self.call(result=item)

    def test_rejects_false_authority_execution_installation_and_qualification(self):
        for field in ('guest_executed', 'installed', 'production_qualified', 'routes_qualified'):
            for value in (True, 0, 'false', None):
                item = response(self.archive); item[field] = value
                with self.subTest(field=field, value=value), self.assertRaises(ValueError): self.call(result=item)
        for field, value in [('authority', 'granted'), ('status', 'rejected'), ('phase', 'execution'),
                             ('preparation', 'runtime_ready'), ('grants_created', False),
                             ('grants_created', 1), ('grants_created', 0.0),
                             ('schema_version', True), ('schema_version', 2), ('diagnostic', 'other'),
                             ('error', {'code': 'failure', 'message': 'failed'}), ('package', None)]:
            item = response(self.archive); item[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError): self.call(result=item)

    def test_missing_extra_fields_and_malformed_limits_are_refused(self):
        for field in response(self.archive):
            item = response(self.archive); del item[field]
            with self.subTest(field=field), self.assertRaises(ValueError): self.call(result=item)
        item = response(self.archive); item['unexpected_authority'] = True
        with self.assertRaises(ValueError): self.call(result=item)
        for location in ('host_limits', 'effective_limits', 'declared_limits'):
            for field in ('fuel', 'memory_bytes', 'host_calls'):
                for bad in (True, '1', None, -1, 1 << 64):
                    item = response(self.archive)
                    (item['package'] if location == 'declared_limits' else item)[location][field] = bad
                    with self.subTest(location=location, field=field, bad=bad), self.assertRaises(ValueError):
                        self.call(result=item)

    def test_non_json_duplicate_keys_nonfinite_and_trailing_data_are_refused(self):
        for raw in (b'not json', b'[]', b'{}{}', b'\xff', b'{"schema_version":1,"schema_version":1}',
                    b'{"nested":{"key":1,"key":2}}', b'{"extra":NaN}', b'{"extra":Infinity}'):
            for discovery_stage in (True, False):
                outputs = [(0, raw)] if discovery_stage else [(0, encoded(advertised())), (0, raw)]
                with self.subTest(raw=raw, discovery_stage=discovery_stage), \
                        mock.patch.object(sdk, 'bounded_process_result', side_effect=outputs):
                    with self.assertRaises(ValueError): sdk.preflight(self.host, self.package)

    def test_changed_host_during_discovery_or_preflight_is_refused(self):
        for phase in ('discovery', 'preflight'):
            self.host.write_bytes(b'unchanged')
            def run(argv, **kwargs):
                if (argv[1] == '--sdk-capabilities') == (phase == 'discovery'):
                    self.host.write_bytes(b'changed!')
                return 0, encoded(advertised() if argv[1] == '--sdk-capabilities' else response(self.archive))
            with self.subTest(phase=phase), mock.patch.object(sdk, 'bounded_process_result', side_effect=run):
                with self.assertRaisesRegex(ValueError, 'host artifact changed'): sdk.preflight(self.host, self.package)

    def test_changed_archive_during_preflight_is_refused(self):
        def run(argv, **kwargs):
            if argv[1] == '--sdk-preflight': self.package.write_bytes(b'replaced')
            return 0, encoded(advertised() if argv[1] == '--sdk-capabilities' else response(self.archive))
        with mock.patch.object(sdk, 'bounded_process_result', side_effect=run):
            with self.assertRaisesRegex(ValueError, 'archive changed'): sdk.preflight(self.host, self.package)

    def test_identical_content_replacement_is_not_the_same_file(self):
        def run(argv, **kwargs):
            if argv[1] == '--sdk-preflight':
                replacement = self.root/'replacement'; replacement.write_bytes(self.archive)
                replacement.replace(self.package)
            return 0, encoded(advertised() if argv[1] == '--sdk-capabilities' else response(self.archive))
        with mock.patch.object(sdk, 'bounded_process_result', side_effect=run):
            with self.assertRaisesRegex(ValueError, 'archive changed'): sdk.preflight(self.host, self.package)

    def test_archive_bound_is_actual_core_bound_and_no_unbounded_read_is_used(self):
        self.assertEqual(sdk.MAX_PACKAGE_BYTES, 4276930)
        with self.package.open('wb') as stream: stream.truncate(sdk.MAX_PACKAGE_BYTES + 1)
        with mock.patch.object(sdk, 'bounded_process', return_value=encoded(advertised())) as run, \
                mock.patch.object(Path, 'read_bytes', side_effect=AssertionError('unbounded read')):
            with self.assertRaisesRegex(ValueError, 'exceeds 4276930'): sdk.preflight(self.host, self.package)
            self.assertEqual(run.call_count, 1)
        with self.package.open('wb') as stream: stream.truncate(sdk.MAX_PACKAGE_BYTES)
        result = sdk.file_snapshot(self.package, 'package', sdk.MAX_PACKAGE_BYTES)
        self.assertEqual(result['bytes'], sdk.MAX_PACKAGE_BYTES)

    def test_archive_growth_during_streaming_is_still_bounded(self):
        with self.package.open('wb') as stream: stream.truncate(sdk.MAX_PACKAGE_BYTES)
        digest = hashlib.sha256()
        first = True
        def update(chunk):
            nonlocal first
            digest.update(chunk)
            if first:
                first = False
                with self.package.open('ab') as stream: stream.write(b'x')
        changing = mock.Mock(update=update, hexdigest=digest.hexdigest)
        with mock.patch.object(sdk.hashlib, 'sha256', return_value=changing):
            with self.assertRaisesRegex(ValueError, 'exceeds 4276930'):
                sdk.file_snapshot(self.package, 'package', sdk.MAX_PACKAGE_BYTES)

    def test_nonregular_selected_package_and_host_are_refused(self):
        folder = self.root/'folder'; folder.mkdir()
        link = self.root/'link'; link.symlink_to(self.package)
        for path in (folder, link):
            with self.subTest(path=path), mock.patch.object(sdk, 'bounded_process', return_value=encoded(advertised())) as run:
                with self.assertRaisesRegex(ValueError, 'regular file'): sdk.preflight(self.host, path)
                self.assertEqual(run.call_count, 1)
            with self.subTest(host=path), mock.patch.object(sdk, 'bounded_process') as run:
                with self.assertRaisesRegex(ValueError, 'regular file'): sdk.preflight(path, self.package)
                run.assert_not_called()

    @unittest.skipUnless(hasattr(os, 'mkfifo') and hasattr(os, 'O_NONBLOCK'), 'POSIX FIFO check')
    def test_fifo_snapshot_refuses_without_waiting_for_a_writer(self):
        fifo = self.root/'fifo'; os.mkfifo(fifo)
        with self.assertRaisesRegex(ValueError, 'regular file'): sdk.file_snapshot(fifo, 'package', sdk.MAX_PACKAGE_BYTES)

    def test_actual_process_preflight_output_limit_and_nonzero_fail_closed(self):
        for stream in ('stdout', 'stderr'):
            with self.subTest(stream=stream):
                output = sdk.bounded_process([sys.executable, '-c',
                    f'import sys;sys.{stream}.buffer.write(b"x"*16384)'], max_output=16384)
                self.assertEqual(len(output), 16384 if stream == 'stdout' else 0)
                with self.assertRaisesRegex(ValueError, 'exceeds 16384'):
                    sdk.bounded_process([sys.executable, '-c',
                        f'import sys;sys.{stream}.buffer.write(b"x"*16385)'], max_output=16384)
        for code in (1, 2, 7):
            with self.subTest(code=code), self.assertRaisesRegex(ValueError, 'exit ' + str(code)):
                sdk.bounded_process([sys.executable, '-c',
                    f'import sys;print({json.dumps(response(self.archive))!r});sys.exit({code})'], max_output=16384)

    def test_broken_pipe_read_fails_closed(self):
        for failure in ('read', 'close'):
            stream = mock.Mock(); stream.read.return_value = b''
            getattr(stream, failure).side_effect = OSError('broken ' + failure)
            process = mock.Mock(stdout=stream, stderr=io.BytesIO(), returncode=0)
            process.poll.return_value = 0
            with self.subTest(failure=failure), mock.patch.object(sdk.subprocess, 'Popen', return_value=process):
                with self.assertRaisesRegex(ValueError, 'pipe read failed'):
                    sdk.bounded_process(['trusted-host', '--sdk-preflight', 'selected'])
            stream.close.assert_called_once_with()
            self.assertTrue(process.stderr.closed)

    @unittest.skipUnless(hasattr(os, 'fork'), 'POSIX inherited pipe check')
    def test_actual_process_inherited_pipe_timeout_is_not_success(self):
        processes = []
        release = self.root/'release-inherited-pipes'
        real_popen = subprocess.Popen
        def start(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            processes.append(process)
            self.addCleanup(process.stdout.close)
            self.addCleanup(process.stderr.close)
            self.addCleanup(release.touch)
            return process
        started = time.monotonic()
        with mock.patch.object(sdk.subprocess, 'Popen', side_effect=start), \
                self.assertRaisesRegex(ValueError, 'pipe did not close'):
            sdk.bounded_process([sys.executable, '-c',
                'import os,sys,time\nfrom pathlib import Path\nchild=os.fork()\n'
                'if child==0:\n'
                ' release=Path(sys.argv[1]); deadline=time.monotonic()+12\n'
                ' while not release.exists() and time.monotonic()<deadline: time.sleep(0.01)\n'
                'os._exit(0)', str(release)])
        self.assertLess(time.monotonic() - started, 8)
        # The caller must return on its existing deadline, but readers must
        # still close their own pipes after the inherited writer finally exits.
        # Release only after return so late cleanup does not depend on timing.
        self.assertFalse(processes[0].stdout.closed)
        self.assertFalse(processes[0].stderr.closed)
        release.touch()
        deadline = time.monotonic() + 2
        while any(not stream.closed for stream in (processes[0].stdout, processes[0].stderr)) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(processes[0].stdout.closed)
        self.assertTrue(processes[0].stderr.closed)

    def test_structured_rejection_phases_preserve_bounded_error_without_claiming_preparation(self):
        for code in ('invalid_arguments', 'invalid_file_type', 'package_unavailable',
                     'package_rejected', 'preparation_rejected', 'output_limit'):
            with self.subTest(code=code):
                rejected = rejection(self.archive, code)
                receipt, _ = self.call(result=rejected, exit_code=2)
                self.assertEqual(receipt['preflight'], rejected)
                self.assertEqual(receipt['archive_sha256'], hashlib.sha256(self.archive).hexdigest())
                self.assertIsNone(receipt['preflight']['effective_limits'])

    def test_exit_status_mismatch_and_unexpected_exit_codes_are_protocol_failures(self):
        for code, value in ((0, rejection(self.archive)), (2, response(self.archive)),
                            (1, response(self.archive)), (7, rejection(self.archive)), (-9, rejection(self.archive))):
            with self.subTest(code=code), self.assertRaisesRegex(ValueError, 'exit'):
                self.call(result=value, exit_code=code)

    def test_rejection_phase_package_and_effective_limits_are_consistent(self):
        for code in ('invalid_arguments', 'invalid_file_type', 'package_unavailable',
                     'package_rejected', 'preparation_rejected', 'output_limit'):
            for field, bad in [('phase', 'execution'), ('effective_limits', response(self.archive)['effective_limits']),
                               ('error', None)]:
                value = rejection(self.archive, code); value[field] = bad
                with self.subTest(code=code, field=field), self.assertRaises(ValueError):
                    self.call(result=value, exit_code=2)
            value = rejection(self.archive, code)
            value['package'] = None if code == 'preparation_rejected' else response(self.archive)['package']
            with self.subTest(code=code, field='package'), self.assertRaises(ValueError):
                self.call(result=value, exit_code=2)

    def test_rejection_error_schema_code_and_message_are_bounded(self):
        for error in (None, [], {}, {'code': 'unknown', 'message': 'Unknown'},
                      {'code': False, 'message': 'Unknown'}, {'code': 'package_rejected', 'message': ''},
                      {'code': 'package_rejected', 'message': 'x'*4097},
                      {'code': 'package_rejected', 'message': 'escaped\x1b[31m'},
                      {'code': 'package_rejected', 'message': 'control\x85'},
                      {'code': 'package_rejected', 'message': 'x', 'authority': 'granted'}):
            value = rejection(self.archive); value['error'] = error
            with self.subTest(error=error), self.assertRaises(ValueError): self.call(result=value, exit_code=2)

    def test_rejections_still_verify_host_archive_and_no_authority(self):
        for field, bad in [('host_version', 'other'), ('authority', 'granted'), ('guest_executed', True),
                           ('installed', True), ('grants_created', 1), ('production_qualified', True)]:
            value = rejection(self.archive); value[field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError): self.call(result=value, exit_code=2)
        value = rejection(self.archive, 'preparation_rejected'); value['package']['archive_sha256'] = '0'*64
        with self.assertRaisesRegex(ValueError, 'archive identity'): self.call(result=value, exit_code=2)

    def test_exit_two_does_not_bypass_json_validation(self):
        for raw in (b'{}', b'\xff', b'[]', b'{"status":"rejected","status":"rejected"}'):
            with self.subTest(raw=raw), mock.patch.object(sdk, 'bounded_process_result', side_effect=[
                    (0, encoded(advertised())), (2, raw)]):
                with self.assertRaises(ValueError): sdk.preflight(self.host, self.package)

    def test_host_option_only_changes_explicit_check_and_never_calls_build_tools(self):
        stdout = io.StringIO()
        with mock.patch.object(tool, 'host_tool', side_effect=AssertionError('repository runtime ran')), \
                mock.patch.object(tool, 'executable', side_effect=AssertionError('compiler searched')), \
                mock.patch.object(sdk, 'preflight', return_value={'preflight': {'status': 'prepared'}}) as preflight, \
                contextlib.redirect_stdout(stdout):
            self.assertEqual(tool.main(['check', str(self.package), '--host', str(self.host)]), 0)
            preflight.assert_called_once_with(str(self.host), self.package)
        self.assertEqual(json.loads(stdout.getvalue()), {'preflight': {'status': 'prepared'}})
        with mock.patch.object(tool, 'host_tool', return_value='legacy check\n') as legacy, \
                mock.patch.object(sdk, 'preflight', side_effect=AssertionError('new route selected')), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(tool.main(['check', str(self.package)]), 0)
            self.assertEqual(legacy.call_args.args[:2], ('plugin_check', ['check', self.package]))

    def test_sdk_only_and_source_distribution_refuse_before_host_or_stdout(self):
        for sdk_only, root in ((True, tool.ROOT), (False, self.root)):
            stdout = io.StringIO(); stderr = io.StringIO()
            with mock.patch.object(tool, 'ROOT', root), \
                    mock.patch.object(sdk, 'preflight', side_effect=AssertionError('host ran')), \
                    contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                argv = ['check', str(self.package), '--host', str(self.host)]
                if sdk_only: argv.append('--sdk-only')
                self.assertEqual(tool.main(argv), 1)
            self.assertEqual(stdout.getvalue(), '')
            self.assertIn('requires trusted', stderr.getvalue())

    def test_cli_protocol_failure_has_no_partial_success_output(self):
        stdout = io.StringIO(); stderr = io.StringIO()
        with mock.patch.object(sdk, 'preflight', side_effect=ValueError('untrusted diagnostic')), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            self.assertEqual(tool.main(['check', str(self.package), '--host', str(self.host)]), 1)
        self.assertEqual(stdout.getvalue(), '')
        self.assertIn('untrusted diagnostic', stderr.getvalue())

    def test_cli_legitimate_rejection_outputs_receipt_and_returns_two(self):
        stdout = io.StringIO(); stderr = io.StringIO()
        receipt = {'preflight': rejection(self.archive)}
        with mock.patch.object(sdk, 'preflight', return_value=receipt), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            self.assertEqual(tool.main(['check', str(self.package), '--host', str(self.host)]), 2)
        self.assertEqual(json.loads(stdout.getvalue()), receipt)
        self.assertEqual(stderr.getvalue(), '')


if __name__ == '__main__':
    unittest.main()
