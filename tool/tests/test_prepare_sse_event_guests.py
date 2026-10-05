"""Fail-closed preparation policy; no compiler or external service required."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

SPEC=importlib.util.spec_from_file_location('sse_prepare',Path(__file__).resolve().parents[1]/'prepare_sse_event_guests.py')
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
            r=Path(td);(r/'network_node_stream_001/schemas').mkdir(parents=True);e=r/'extension';(e/'contracts').mkdir(parents=True)
            data=(M.EXT/'contracts/sse_event.capnp').read_bytes()
            for altered in [data.replace(b'\n',b'\r\n'),data+b'\n']:
                (r/'network_node_stream_001/schemas/sse_event.capnp').write_bytes(altered)
                (e/'contracts/sse_event.capnp').write_bytes(altered)
                with patch.object(M,'ROOT',r),patch.object(M,'EXT',e),self.assertRaises(ValueError):M.validate_contract()
            (r/'network_node_stream_001/schemas/sse_event.capnp').write_bytes(data)
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
            with patch.object(M,'os',SimpleNamespace(name='nt',path=M.os.path,lstat=M.os.lstat)):
                self.assertEqual(M.compiler_paths(root),(root/'bin/clang.exe',root/'bin/clang++.exe',root/'share/wasi-sysroot'))
            with patch.object(M,'os',SimpleNamespace(name='posix',path=M.os.path,lstat=M.os.lstat)):
                self.assertEqual(M.compiler_paths(root),(root/'bin/clang',root/'bin/clang++',root/'share/wasi-sysroot'))

    def test_output_and_targets_refuse_equal_or_nested_paths(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td)
            for output,targets in [(root,[root,root/'other']), (root,[root/'inside',root/'other']),
                                   (root/'out',[root,root/'other']), (root/'out',[root/'a',root/'a']),
                                   (root/'out',[root/'a',root/'a/inside'])]:
                with self.subTest(output=output,targets=targets),self.assertRaises(ValueError):M.disjoint_paths(output,targets)
            M.disjoint_paths(root/'out',[root/'rust',root/'c'])

    def test_dependencies_match_exact_original_lock_and_reject_checksum_or_source_drift(self):
        M.validate_dependencies()
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'sdk/rust').mkdir(parents=True);ext=root/'extension';ext.mkdir()
            good='[[package]]\nname="codec"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="exact"\n'
            (root/'sdk/rust/Cargo.lock').write_text(good)
            for bad in [good.replace('exact','wrong'),good.replace('1.0.0','1.0.1'),good.replace('registry+','git+')]:
                (ext/'Cargo.lock').write_text(bad)
                with patch.object(M,'ROOT',root),patch.object(M,'EXT',ext),self.assertRaises(ValueError):M.validate_dependencies()
            (ext/'Cargo.lock').write_text(good)
            with patch.object(M,'ROOT',root),patch.object(M,'EXT',ext):M.validate_dependencies()

    def test_reparse_ancestor_refused_before_output_resolution(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);original=M.os.lstat
            def metadata(path,*args,**kwargs):
                value=original(path,*args,**kwargs)
                if Path(path)==root:
                    return SimpleNamespace(st_mode=value.st_mode,st_file_attributes=0x400)
                return value
            with patch.object(M.os,'lstat',metadata),self.assertRaisesRegex(ValueError,'reparse'):
                M.external(root/'missing-output')

    def test_sysroot_snapshot_covers_headers_libraries_and_refuses_reparse_inputs(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'include').mkdir();(root/'lib').mkdir()
            header=root/'include/example.h';library=root/'lib/libc.a'
            header.write_bytes(b'header');library.write_bytes(b'library')
            before=M.sysroot_snapshot(root)
            self.assertEqual(set(before),{'include/example.h','lib/libc.a'})
            library.write_bytes(b'changed-library');self.assertNotEqual(before,M.sysroot_snapshot(root))
            original=M.os.lstat
            def metadata(path,*args,**kwargs):
                value=original(path,*args,**kwargs)
                if Path(path)==library:return SimpleNamespace(st_mode=value.st_mode,st_file_attributes=0x400)
                return value
            with patch.object(M.os,'lstat',metadata),self.assertRaisesRegex(ValueError,'reparse'):
                M.sysroot_snapshot(root)

if __name__=='__main__':unittest.main()
