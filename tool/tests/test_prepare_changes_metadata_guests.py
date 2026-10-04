"""Fail-closed preparation policy; no compiler or external service required."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC=importlib.util.spec_from_file_location('changes_prepare',Path(__file__).resolve().parents[1]/'prepare_changes_metadata_guests.py')
M=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(M)

class PreparationTests(unittest.TestCase):
    def test_frozen_contract_exact_bytes(self):
        M.validate_contract()
    def test_repository_output_forbidden(self):
        for path in [M.ROOT,M.ROOT/'sdk',M.EXT/'target',M.ROOT/'tool/../sdk']:
            with self.assertRaises(ValueError):M.external(path)
    def test_existing_output_never_overwritten(self):
        with tempfile.TemporaryDirectory() as td:
            output=Path(td)/'evidence';output.mkdir();sentinel=output/'plugin.wasm';sentinel.write_bytes(b'original')
            args=['prepare','--output',str(output),'--rust-target',td+'/rust','--c-target',td+'/c','--wasi-sdk',td,'--pack-executable',td+'/missing']
            with patch('sys.argv',args),self.assertRaises(ValueError):M.main()
            self.assertEqual(sentinel.read_bytes(),b'original')
    def test_changed_or_crlf_contract_rejected_without_normalization(self):
        with tempfile.TemporaryDirectory() as td:
            r=Path(td);(r/'core/schemas').mkdir(parents=True);e=r/'extension';(e/'contracts').mkdir(parents=True)
            data=(M.EXT/'contracts/changes_metadata_v1.wire').read_bytes()
            for altered in [data.replace(b'\n',b'\r\n'),data+b'\n']:
                (r/'core/schemas/changes_metadata_v1.wire').write_bytes(altered)
                (e/'contracts/changes_metadata_v1.wire').write_bytes(altered)
                with patch.object(M,'ROOT',r),patch.object(M,'EXT',e),self.assertRaises(ValueError):M.validate_contract()
            (r/'core/schemas/changes_metadata_v1.wire').write_bytes(data)
            with patch.object(M,'ROOT',r),patch.object(M,'EXT',e),self.assertRaises(ValueError):M.validate_contract()

if __name__=='__main__':unittest.main()
