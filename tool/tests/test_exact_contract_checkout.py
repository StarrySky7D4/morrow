"""Raw protocol contracts must survive Windows Git newline conversion."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
CONTRACTS = {
    "extensions/blob-transfer-v1/contracts/blob_transfer.capnp":
        "941db7c662815f8963b46bbb65a5c143d90f9e7217937e84c05d1b8f11024543",
    "extensions/blob-upload-proposal-v1/contracts/blob_upload_proposal.capnp":
        "b06b2c578b65c4f8bdf8af8cd3ca4976b42fa6da5f6c608373b453ae4f710de2",
    "extensions/blob-upload-request-v1/contracts/blob_upload_request.capnp":
        "07220eb1f92cbd43bb369b51d945ecbf428e586d6ac59a0c0ecae0bbdd44b0df",
    "extensions/sse-event-v1/contracts/sse_event.capnp":
        "bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863",
    "extensions/ws-message-v1/contracts/ws_message.capnp":
        "2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d",
    "sdk/examples/blob-upload-common/contracts/sample_options.capnp":
        "437bbb31800ec52aff91c409a2d97102daf7269f4144f54ed5b6abff3438ff6e",
}


class ExactContractCheckoutTest(unittest.TestCase):
    def check_contracts(self, root):
        for relative, expected in CONTRACTS.items():
            with self.subTest(contract=relative):
                self.assertEqual(
                    hashlib.sha256((root / relative).read_bytes()).hexdigest(),
                    expected,
                )

    def test_working_contracts_match_fixed_protocol_digests(self):
        self.check_contracts(ROOT)

    def checkout_contracts(self, autocrlf):
        with tempfile.TemporaryDirectory(prefix="morrow-contract-checkout-") as temp:
            root = Path(temp)
            repo, output = root / "repo", root / "checkout"
            repo.mkdir()
            output.mkdir()
            shutil.copyfile(ROOT / ".gitattributes", repo / ".gitattributes")
            for relative in CONTRACTS:
                target = repo / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, target)

            def git(*args):
                subprocess.run(
                    ["git", "-c", f"core.autocrlf={autocrlf}", *args],
                    cwd=repo, check=True, capture_output=True,
                )

            git("init", "--quiet")
            git("add", "--", ".gitattributes", *CONTRACTS)
            git("checkout-index", "--all", f"--prefix={output.as_posix()}/")
            self.check_contracts(output)

    def test_windows_autocrlf_checkout_preserves_raw_contracts(self):
        self.checkout_contracts("true")

    def test_lf_checkout_preserves_raw_contracts(self):
        self.checkout_contracts("false")


if __name__ == "__main__":
    unittest.main()
