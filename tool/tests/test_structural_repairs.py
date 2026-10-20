import importlib.util
from pathlib import Path
import unittest

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

if __name__=='__main__': unittest.main()
