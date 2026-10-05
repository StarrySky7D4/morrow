"""Standalone entry-point tests; synthetic hosts are protocol fixtures only."""
import contextlib
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

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tool'))
import morrow_plugin
import morrow_sdk_diagnostics as cli
import package_plugin_sdk
import package_sdk_diagnostics as distribution
import sdk_profiles
from test_sdk_preflight import advertised, response, rejection
from test_sdk_profiles import descriptor


class DiagnosticsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.bundle = self.root / 'standalone'
        self.bundle.mkdir()
        archive = self.root / 'diagnostics.zip'
        distribution.create_archive(ROOT, archive)
        distribution.verify_zip(archive)
        import zipfile
        with zipfile.ZipFile(archive) as zipped:
            for name in zipped.namelist():
                (self.bundle / name).write_bytes(zipped.read(name))
        distribution.verify_directory(self.bundle)
        self.script = self.bundle / 'morrow_sdk_diagnostics.py'
        self.package = self.root / 'selected.mplugin'
        self.package.write_bytes(b'selected synthetic archive')
        self.calls = self.root / 'calls.jsonl'
        self.host = self.root / ('trusted-host.cmd' if os.name == 'nt' else 'trusted-host')
        self.cwd, self.home, self.tmp = (self.root / name for name in ('cwd', 'home', 'tmp'))
        for path in (self.cwd, self.home, self.tmp):
            path.mkdir()
        self.env = {key: value for key, value in os.environ.items() if key not in ('PYTHONPATH', 'PYTHONHOME')}
        self.env.update(PATH='', HOME=str(self.home), TMPDIR=str(self.tmp))

    def host_script(self, discovery=None, result=None, exit_code=0, raw=None, extra=''):
        discovery = advertised() if discovery is None else discovery
        result = response(self.package.read_bytes()) if result is None else result
        script = self.host.with_suffix('.py') if os.name == 'nt' else self.host
        script.write_text('#!' + sys.executable + '\n' +
            'import json, sys, time\nfrom pathlib import Path\n' +
            'with Path(' + repr(str(self.calls)) + ').open("a") as out: out.write(json.dumps(sys.argv[1:])+"\\n")\n' +
            'if sys.argv[1:] == ["--sdk-capabilities"]:\n' +
            ' print(' + repr(json.dumps(discovery)) + ')\n raise SystemExit(0)\n' + extra + '\n' +
            'sys.stdout.buffer.write(' + repr(json.dumps(result).encode() if raw is None else raw) + ')\n' +
            'raise SystemExit(' + str(exit_code) + ')\n', encoding='utf-8')
        if os.name == 'nt':
            self.host.write_text('@echo off\n"' + sys.executable + '" "' + str(script) + '" %*\n', encoding='utf-8')
        else:
            self.host.chmod(0o755)

    def run_cli(self, args, isolated=True):
        argv = [sys.executable] + (['-I', '-B'] if isolated else []) + [str(self.script)] + list(map(str, args))
        return subprocess.run(argv, cwd=self.cwd, env=self.env, capture_output=True, timeout=12)

    def preflight(self):
        return self.run_cli(['preflight', self.package, '--host', self.host])

    def assert_failure(self, result, message=None):
        self.assertEqual(result.returncode, 1, result)
        self.assertEqual(result.stdout, b'')
        self.assertTrue(result.stderr.startswith(b'ERROR:'), result.stderr)
        if message:
            self.assertIn(message.encode(), result.stderr)

    def test_entry_point_delegates_to_original_adjacent_consumer(self):
        sdk = cli.load_profiles()
        self.assertEqual(Path(sdk.__file__), ROOT / 'tool/sdk_profiles.py')
        self.assertTrue(sys.dont_write_bytecode)
        for args, function, receipt, expected in (
                (['profiles', '--host', '/trusted'], 'query', {'descriptor': {}}, 0),
                (['preflight', '/package', '--host', '/trusted'], 'preflight', {'preflight': {'status': 'prepared'}}, 0),
                (['preflight', '/package', '--host', '/trusted'], 'preflight', {'preflight': {'status': 'rejected'}}, 2)):
            with self.subTest(function=function, expected=expected), mock.patch.object(cli, 'load_profiles', return_value=sdk), \
                    mock.patch.object(sdk, function, return_value=receipt) as call, \
                    contextlib.redirect_stdout(io.StringIO()) as stdout:
                self.assertEqual(cli.main(args), expected)
                self.assertEqual(json.loads(stdout.getvalue()), receipt)
                call.assert_called_once_with(*([Path('/trusted')] + ([Path('/package')] if function == 'preflight' else [])))

    def test_isolated_profiles_prepared_and_rejected_actual_stdout_exit(self):
        self.host_script()
        result = self.run_cli(['profiles', '--host', self.host])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['descriptor'], advertised())
        result = self.preflight()
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt['preflight'], response(self.package.read_bytes()))
        self.assertFalse(result.stderr)
        self.host_script(result=rejection(self.package.read_bytes()), exit_code=2)
        result = self.preflight()
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(json.loads(result.stdout)['preflight']['status'], 'rejected')
        self.assertFalse(result.stderr)

    def test_legacy_profiles_still_work_preflight_refuses_before_new_flag_or_package_read(self):
        self.host_script(discovery=descriptor())
        self.assertEqual(self.run_cli(['profiles', '--host', self.host]).returncode, 0)
        self.calls.unlink()
        result = self.run_cli(['preflight', self.root / 'missing', '--host', self.host])
        self.assert_failure(result, 'does not advertise')
        self.assertEqual([json.loads(line) for line in self.calls.read_text().splitlines()], [['--sdk-capabilities']])

    def test_protocol_and_process_failure_never_emit_receipt(self):
        for raw, code in ((b'not json', 0), (b'{}{}', 0), (b'{"x":1,"x":1}', 0),
                          (b'{"x":NaN}', 0), (b'\xff', 0), (None, 7), (None, 2)):
            with self.subTest(raw=raw, code=code):
                self.host_script(raw=raw, exit_code=code)
                self.assert_failure(self.preflight())
        for field, value in (('guest_executed', True), ('authority', 'granted'),
                             ('grants_created', 1), ('production_qualified', True)):
            bad = response(self.package.read_bytes())
            bad[field] = value
            self.host_script(result=bad)
            self.assert_failure(self.preflight())

    def test_local_input_failures_are_exit_one_without_stdout(self):
        self.host_script()
        self.assert_failure(self.run_cli(['profiles', '--host', self.root / 'missing']))
        self.assert_failure(self.run_cli(['preflight', self.root / 'missing', '--host', self.host]))
        self.assert_failure(self.run_cli(['preflight', self.cwd, '--host', self.host]))
        if os.name == 'nt':
            self.host = self.root / 'invalid-host.exe'
            self.host.write_bytes(b'not a Windows executable')
        else:
            self.host.chmod(0o644)
        self.assert_failure(self.preflight())

    def test_no_fallback_or_implicit_commands_and_usage_is_not_a_receipt(self):
        self.host_script()
        for args in ([], ['profiles'], ['preflight', self.package], ['check', self.package, '--host', self.host],
                     ['profiles', '--host', self.host, '--sdk-only']):
            with self.subTest(args=args):
                result = self.run_cli(args)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, b'')
                self.assertIn(b'usage:', result.stderr)
        self.assertFalse(self.calls.exists())
        (self.bundle / 'sdk_profiles.py').unlink()
        self.env['PYTHONPATH'] = str(ROOT / 'tool')
        self.assert_failure(self.run_cli(['profiles', '--host', self.host], isolated=False), 'sdk_profiles.py')
        self.assertFalse(self.calls.exists())

    def test_ordinary_invocation_writes_no_bytecode_or_working_state(self):
        self.host_script()
        result = self.run_cli(['profiles', '--host', self.host], isolated=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual({p.name for p in self.bundle.iterdir()}, distribution.PAYLOAD | {distribution.MANIFEST})
        distribution.verify_directory(self.bundle)
        for path in (self.cwd, self.home, self.tmp):
            self.assertEqual(list(path.iterdir()), [])

    def test_original_process_bounds_are_retained(self):
        self.host_script(raw=b'x' * (sdk_profiles.PREFLIGHT_MAX_OUTPUT + 1))
        self.assert_failure(self.preflight(), 'exceeds')
        if os.name == 'nt':
            # The .cmd fixture owns a Python child. The consumer only promises
            # bounded return, not process-tree cleanup, so release our fixture
            # after observing that return and move its CWD before test cleanup.
            release, done = self.root / 'release', self.root / 'done'
            extra = ('while not Path(' + repr(str(release)) + ').exists(): time.sleep(.01)\n'
                     'import os\nos.chdir(' + repr(str(self.root.parent)) + ')\n'
                     'Path(' + repr(str(done)) + ').write_text("released")')
        else:
            extra = 'time.sleep(30)'
        self.host_script(extra=extra)
        start = time.monotonic()
        try:
            self.assert_failure(self.preflight(), 'timed out')
            self.assertLess(time.monotonic() - start, 8)
        finally:
            if os.name == 'nt':
                release.write_text('release only our synthetic fixture')
                deadline = time.monotonic() + 3
                while not done.exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue(done.exists(), 'synthetic child did not release its CWD')

    def test_old_sdk_only_gate_and_exporter_closure_stay_closed(self):
        self.assertEqual(package_plugin_sdk.TOOLS, ('morrow_plugin.py', 'plugin_sdk_lock.py', 'package_plugin_sdk.py'))
        for command in ('pack', 'check', 'transform'):
            args = [command, str(self.package), '--sdk-only']
            if command == 'check':
                args += ['--host', str(self.host)]
            if command == 'transform':
                args += ['handler', 'input', 'output', 'in.file', 'out.file']
            with self.subTest(command=command), mock.patch.object(subprocess, 'Popen', side_effect=AssertionError('child attempted')), \
                    contextlib.redirect_stdout(io.StringIO()) as stdout, contextlib.redirect_stderr(io.StringIO()) as stderr:
                self.assertEqual(morrow_plugin.main(args), 1)
                self.assertEqual(stdout.getvalue(), '')
                self.assertIn('SDK-only mode refuses', stderr.getvalue())


if __name__ == '__main__':
    unittest.main()
