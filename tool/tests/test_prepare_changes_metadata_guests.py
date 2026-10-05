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

    def test_explicit_windows_paths_with_spaces_are_preserved(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)/'LLVM with spaces';root.mkdir()
            cc=root/'clang.exe';cxx=root/'clang++.exe';cc.write_bytes(b'compiler-c');cxx.write_bytes(b'compiler-cpp')
            sysroot=Path(td)/'WASI sysroot';sysroot.mkdir()
            self.assertEqual(M.compiler_paths(td,cc,cxx,sysroot),(cc.resolve(),cxx.resolve(),sysroot.resolve()))
    def test_partial_explicit_route_or_missing_input_fails_closed(self):
        with tempfile.TemporaryDirectory() as td:
            cc=Path(td)/'clang.exe';cc.write_bytes(b'synthetic')
            for inputs in [(cc,None,None),(None,cc,None),(None,None,td),(cc,cc,None),(cc,None,td),(None,cc,td)]:
                with self.assertRaises(ValueError):M.compiler_paths(td,*inputs)
            with self.assertRaises(FileNotFoundError):M.compiler_paths(td,cc,Path(td)/'missing.exe',td)
            with self.assertRaises(FileNotFoundError):M.compiler_paths(td,cc,cc,Path(td)/'missing-sysroot')
    def test_existing_sdk_layout_keeps_defaults_and_windows_exe_selection(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'bin').mkdir();(root/'share/wasi-sysroot').mkdir(parents=True)
            for name in ['clang','clang++','clang.exe','clang++.exe']:(root/'bin'/name).write_bytes(b'synthetic')
            with patch.object(M,'os',type('OS',(),{'name':'nt'})()):
                self.assertEqual(M.compiler_paths(root),(root/'bin/clang.exe',root/'bin/clang++.exe',root/'share/wasi-sysroot'))
            with patch.object(M,'os',type('OS',(),{'name':'posix'})()):
                self.assertEqual(M.compiler_paths(root),(root/'bin/clang',root/'bin/clang++',root/'share/wasi-sysroot'))

if __name__=='__main__':unittest.main()
