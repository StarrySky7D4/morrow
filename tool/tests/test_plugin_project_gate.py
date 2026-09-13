"""Negative qualifications must prove the intended failure, not merely exit nonzero."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_plugin_projects import check_result


CASES = {
    "no-clobber": ("output already exists; refusing to overwrite", "(exit code: 5)"),
    "business": ("BUSINESS_FAILURE code=unsupportedInput", "(exit code: 3)"),
    "compile": ("error: intentional qualification failure", "failed (exit 101)"),
    "bad-package": ('Error: Invalid("container header")', "PREPARED: checked only;", " install "),
}


class ProjectQualificationGateTests(unittest.TestCase):
    def test_missing_cargo_cannot_satisfy_any_expected_negative_case(self):
        diagnostic = "ERROR: missing tool: cargo\n"
        for name, required in CASES.items():
            with self.subTest(case=name):
                with self.assertRaisesRegex(RuntimeError, "expected diagnostic missing"):
                    check_result(1, diagnostic, success=False, required=required)

    def test_exact_target_diagnostics_and_nonzero_exit_are_accepted(self):
        for name, required in CASES.items():
            with self.subTest(case=name):
                diagnostic = "qualification output\n" + "\n".join(required) + "\n"
                self.assertIsNone(check_result(1, diagnostic, success=False, required=required))

    def test_each_marker_is_required_including_failure_class_and_install_stage(self):
        for name, required in CASES.items():
            for missing in required:
                with self.subTest(case=name, missing=missing):
                    diagnostic = "\n".join(marker for marker in required if marker != missing)
                    with self.assertRaisesRegex(RuntimeError, "expected diagnostic missing"):
                        check_result(1, diagnostic, success=False, required=required)
        with self.assertRaisesRegex(RuntimeError, "expected diagnostic missing"):
            check_result(1, "BUSINESS_FAILURE code=unsupportedInput\n(exit code: 5)",
                         success=False, required=CASES["business"])

    def test_success_never_accepts_nonzero_even_with_success_marker(self):
        self.assertIsNone(check_result(0, "PREPARED", required=("PREPARED",)))
        for status in (1, 101, -1):
            with self.subTest(exit=status):
                with self.assertRaisesRegex(RuntimeError, "unexpected exit"):
                    check_result(status, "PREPARED", success=True, required=("PREPARED",))
        with self.assertRaisesRegex(RuntimeError, "expected diagnostic missing"):
            check_result(0, "unrelated output", required=("PREPARED",))

    def test_negative_case_rejects_zero_exit_even_with_expected_error_text(self):
        for name, required in CASES.items():
            with self.subTest(case=name):
                with self.assertRaisesRegex(RuntimeError, "unexpected exit 0"):
                    check_result(0, "\n".join(required), success=False, required=required)

    def test_negative_case_without_diagnostic_contract_is_invalid(self):
        for required in ((), []):
            with self.subTest(required=required):
                with self.assertRaisesRegex(ValueError, "must specify its expected diagnostic"):
                    check_result(1, "any failure", success=False, required=required)


if __name__ == "__main__":
    unittest.main()
