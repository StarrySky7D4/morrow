import importlib.util
from pathlib import Path
import unittest
import tempfile
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location('repairs',Path(__file__).resolve().parents[1]/'verify_structural_repairs.py')
repairs=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(repairs)

class QualificationGateTests(unittest.TestCase):
    def receipts(self):
        return [dict(gate=g,commit='head',source_digest='source',complete=True,status='PASS') for g in repairs.REQUIRED]
    def test_old_commit_or_changed_source_does_not_qualify(self):
        self.assertEqual(repairs.remaining_gates(self.receipts(),'next','source'),list(repairs.REQUIRED))
        self.assertEqual(repairs.remaining_gates(self.receipts(),'head','changed'),list(repairs.REQUIRED))
    def test_partial_or_not_run_is_never_pass(self):
        rows=self.receipts();rows[0]['complete']=False;rows[1]['status']='NOT_RUN'
        self.assertEqual(repairs.remaining_gates(rows,'head','source'),list(repairs.REQUIRED[:2]))
    def test_every_named_tier_is_required(self):
        rows=self.receipts()
        self.assertEqual(repairs.remaining_gates(rows,'head','source'),[])
        self.assertEqual(repairs.remaining_gates(rows[:1],'head','source'),list(repairs.REQUIRED[1:]))
    def test_portable_checks_do_not_issue_runtime_receipts_or_run_windows_account_test(self):
        steps=repairs.commands('portable',Path('/tmp/qualification'))
        self.assertNotIn('L0_TOKEN',[name for name,_ in steps])
        self.assertFalse(any('--ignored' in cmd for _,cmd in steps))

class NativeFixtureEvidenceTests(unittest.TestCase):
    def test_run_temp_is_isolated_without_changing_user_or_cargo_configuration(self):
        with tempfile.TemporaryDirectory() as directory:
            original = repairs.os.environ.copy()
            output = Path(directory)
            environment = repairs.command_environment(output)
            self.assertTrue((output / 'tmp').is_dir())
            for name in ('TMPDIR', 'TMP', 'TEMP'):
                self.assertEqual(environment[name], str(output / 'tmp'))
            for name in ('HOME', 'USERPROFILE', 'CARGO_HOME', 'RUSTUP_HOME'):
                self.assertEqual(environment.get(name), original.get(name))
            self.assertEqual(dict(repairs.os.environ), original)

    def test_only_synthetic_fixture_environment_uses_empty_cargo_home(self):
        with tempfile.TemporaryDirectory() as directory:
            original = {'CARGO_HOME': 'actual-cache', 'RUSTUP_HOME': 'actual-toolchain'}
            isolated = repairs.fixture_environment(original, Path(directory))
            self.assertTrue(Path(isolated['CARGO_HOME']).is_dir())
            self.assertEqual(list(Path(isolated['CARGO_HOME']).iterdir()), [])
            self.assertEqual(isolated['RUSTUP_HOME'], original['RUSTUP_HOME'])
            self.assertEqual(original['CARGO_HOME'], 'actual-cache')

    def test_l0_requires_single_fully_qualified_discovered_test(self):
        self.assertTrue(repairs.l0_discovered(f'{repairs.L0_TEST}: test\n\n1 test, 0 benchmarks\n'))
        for evidence in ('0 tests, 0 benchmarks\n',
                         'other::fixture: test\n\n1 test, 0 benchmarks\n',
                         f'{repairs.L0_TEST}: test\nother: test\n\n2 tests, 0 benchmarks\n',
                         f'{repairs.L0_TEST}: test\n\n0 tests, 0 benchmarks\n'):
            self.assertFalse(repairs.l0_discovered(evidence))

    def test_l0_receipt_requires_named_execution_and_exact_count(self):
        summary = 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 121 filtered out; finished in 0.02s\n'
        evidence = f'running 1 test\ntest {repairs.L0_TEST} ... ok\n\n{summary}'
        self.assertTrue(repairs.l0_executed(evidence))
        for invalid in (summary, evidence.replace(repairs.L0_TEST, 'another::test'),
                        evidence.replace('1 passed', '0 passed'),
                        evidence.replace('0 failed', '1 failed'),
                        evidence.replace('0 ignored', '1 ignored'),
                        evidence.replace('... ok', '... ignored'), evidence + summary):
            self.assertFalse(repairs.l0_executed(invalid))

    def test_discovery_precedes_disposable_fixture_with_same_exact_filter(self):
        output = Path('qualification')
        with patch.object(repairs.os, 'name', 'nt'), patch.dict(repairs.os.environ, {'MORROW_DISPOSABLE_WINDOWS_TEST': '1'}):
            steps = repairs.commands('windows', output, True)
        self.assertEqual([name for name, _ in steps][-2:], ['L0_TOKEN_DISCOVERY', 'L0_TOKEN'])
        for _, command in steps[-2:]:
            self.assertIn(repairs.L0_TEST, command)
            self.assertIn('--exact', command)
            self.assertIn('--ignored', command)
        self.assertIn('--list', steps[-2][1])
        self.assertNotIn('--list', steps[-1][1])

    def test_windows_default_never_runs_account_fixture(self):
        output = Path('qualification')
        with patch.object(repairs.os, 'name', 'nt'):
            steps = repairs.commands('windows', output)
        self.assertFalse(any('--ignored' in command for _, command in steps))

    def test_windows_host_tests_build_all_real_guests_before_running(self):
        with patch.object(repairs.os, 'name', 'nt'):
            steps = repairs.commands('portable', Path('qualification'))
        names = [name for name, _ in steps]
        for variable, manifest, filename in repairs.WINDOWS_HOST_FIXTURES:
            index = names.index(variable + ' build')
            self.assertLess(index, names.index('host unit tests'))
            command = steps[index][1]
            self.assertIn('--locked', command)
            self.assertIn(manifest, command)
            self.assertIn('wasm32-unknown-unknown', command)
            self.assertIn(repairs.WASM_TARGET, command)
            self.assertTrue(filename.endswith('.wasm'))
        self.assertFalse(any('--ignored' in command for _, command in steps))

if __name__=='__main__': unittest.main()
