#!/usr/bin/env python3
"""Build NEW metadata guests offline in an external evidence directory.

Only explicitly supplied external Cargo caches are reused. Never rebuild/repack
old guest artifacts or mutate old SDK distributions, contracts, pins or locks.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

PROFILE = '07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089'
ROOT = Path(__file__).resolve().parents[1]
EXT = ROOT / 'extensions/changes-metadata-v1'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def external(path):
    value = Path(path).resolve()
    if value == ROOT or ROOT in value.parents:
        raise ValueError('outputs and target caches must be external to the source repository')
    return value


def validate_contract():
    core = ROOT / 'core/schemas/changes_metadata_v1.wire'
    sdk = EXT / 'contracts/changes_metadata_v1.wire'
    if core.read_bytes() != sdk.read_bytes() or sha(sdk) != PROFILE:
        raise ValueError('Core/extension contract bytes or frozen profile digest mismatch')


def compiler_paths(wasi, clang=None, clangxx=None, sysroot=None):
    """An explicit Windows LLVM/sysroot route is complete or fails closed."""
    supplied = (clang is not None, clangxx is not None, sysroot is not None)
    if any(supplied) and not all(supplied):
        raise ValueError('explicit compiler route requires --clang, --clangxx and --sysroot together')
    wasi = Path(wasi).resolve(strict=True)
    def executable(name):
        plain = wasi / 'bin' / name
        windows = plain.with_suffix('.exe')
        return windows if os.name == 'nt' and windows.is_file() else plain
    cc = Path(clang) if all(supplied) else executable('clang')
    cxx = Path(clangxx) if all(supplied) else executable('clang++')
    root = Path(sysroot) if all(supplied) else wasi / 'share/wasi-sysroot'
    cc, cxx, root = (x.resolve(strict=True) for x in (cc, cxx, root))
    if not cc.is_file() or not cxx.is_file() or not root.is_dir():
        raise ValueError('compiler files and sysroot directory are required')
    return cc, cxx, root


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', required=True)
    p.add_argument('--rust-target', required=True)
    p.add_argument('--c-target', required=True)
    p.add_argument('--wasi-sdk', required=True)
    p.add_argument('--pack-executable', required=True)
    p.add_argument('--clang', help='explicit C compiler; requires --clangxx and --sysroot')
    p.add_argument('--clangxx', help='explicit C++ compiler; requires --clang and --sysroot')
    p.add_argument('--sysroot', help='explicit WASI sysroot; requires both compilers')
    args = p.parse_args()
    validate_contract()
    original={p['name']:(p['version'],p.get('checksum')) for p in tomllib.loads((ROOT/'sdk/rust/Cargo.lock').read_text())['package'] if 'source' in p}
    for dependency in tomllib.loads((EXT/'Cargo.lock').read_text())['package']:
        if 'source' in dependency and original.get(dependency['name'])!=(dependency['version'],dependency.get('checksum')):
            raise ValueError('extension dependency does not match original exact cached SDK lock: '+dependency['name'])
    output = external(args.output)
    targets = [external(args.rust_target), external(args.c_target)]
    if output.exists():
        raise ValueError('output must not already exist; refusing overwrite')
    wasi = Path(args.wasi_sdk).resolve(strict=True)
    pack = Path(args.pack_executable).resolve(strict=True)
    clang, clangxx, sysroot = compiler_paths(wasi, args.clang, args.clangxx, args.sysroot)
    for path in targets:
        if path == output or output in path.parents:
            raise ValueError('reusable cache must not be inside output')
    source_paths = sorted(x for x in EXT.rglob('*') if x.is_file() and 'target' not in x.parts)
    source_paths += [Path(__file__).resolve(), ROOT / 'core/examples/changes_metadata_package.rs']
    before = {str(x.relative_to(ROOT)): sha(x) for x in source_paths}
    # Include exact original transport source bytes; copying a compiler cache is
    # not a claim that any historical guest has been requalified.
    transport = {str(x.relative_to(ROOT)): sha(x) for x in sorted((ROOT/'sdk').rglob('*')) if x.is_file()}
    output.mkdir(parents=True, exist_ok=False)
    manifest = {'profile': PROFILE, 'sources': before, 'transport_sources': transport,
                'pack_executable': {'path': str(pack), 'sha256': sha(pack)},
                'targets': list(map(str,targets)), 'commands': [], 'artifacts': {}, 'status': 'building'}
    def save():
        (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    def run(command, target):
        command = list(map(str,command))
        env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_NET_OFFLINE='true')
        manifest['commands'].append(command); save()
        with (output/'commands.log').open('ab') as log:
            log.write(('\n$ '+' '.join(command)+'\n').encode()); log.flush()
            subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    try:
        toolchains = {}
        for name, command in [('rustc',['rustc','--version']),('cargo',['cargo','--version']),('capnp',['capnp','--version']),('clang',[clang,'--version']),('clangxx',[clangxx,'--version'])]:
            executable=Path(shutil.which(str(command[0])) or command[0]).resolve(strict=True)
            toolchains[name] = {'version':subprocess.check_output(list(map(str,command)),text=True).strip(),'path':str(executable),'sha256':sha(executable)}
        manifest['toolchains']=toolchains; save()
        cargo=['cargo','build','--locked','--offline','--manifest-path',EXT/'Cargo.toml','--target','wasm32-unknown-unknown','--release']
        run(cargo+['-p','morrow-changes-metadata-guest','--target-dir',targets[0]],targets[0])
        rust_wasm=targets[0]/'wasm32-unknown-unknown/release/morrow_changes_metadata_guest.wasm'
        run(cargo+['-p','morrow-changes-metadata-v1','--features','c-transport','--target-dir',targets[1]],targets[1])
        library=targets[1]/'wasm32-unknown-unknown/release/libmorrow_changes_metadata_v1.a'
        manifest['combined_library']={'path':str(library),'sha256':sha(library)}; save()
        manifest['sysroot'] = str(sysroot); save()
        common=['--target=wasm32-wasip1','--sysroot='+str(sysroot),'-O2','-Wall','-Wextra','-Werror','-I'+str(ROOT/'sdk/c/include'),'-I'+str(EXT/'include')]
        objects=[]
        for name in ['morrow_plugin_sdk','morrow_plugin_wasm','morrow_plugin_wasm_libc','morrow_plugin_task','morrow_channel_v1']:
            obj=output/(name+'.o');run([clang,*common,'-std=c11','-c',ROOT/f'sdk/c/src/{name}.c','-o',obj],targets[1]); objects.append(obj)
        link=['-nostdlib','-Wl,--no-entry','-Wl,--export=morrow_run','-Wl,-z,stack-size=1048576','-Wl,--max-memory=16777216','-Wl,--strip-all']
        stdlib=sysroot/'lib/wasm32-wasip1'
        for language in ['rust','c','cpp']:
            destination=output/language;destination.mkdir();module=destination/'plugin.wasm'
            if language=='rust':
                shutil.copyfile(rust_wasm,module)
            elif language=='c':
                run([clang,*common,'-std=c11',EXT/'guests/c/plugin.c',*objects,library,*link,'-L'+str(stdlib),'-lc','-o',module],targets[1])
            else:
                cppcommon=[*common,'-std=c++17','-nostdinc++','-isystem',sysroot/'include/wasm32-wasip1/noeh/c++/v1','-fno-exceptions','-fno-rtti','-I'+str(ROOT/'sdk/cpp/include')]
                runtime=output/'morrow_plugin_wasm_runtime.o'
                run([clangxx,*cppcommon,'-c',ROOT/'sdk/cpp/src/morrow_plugin_wasm_runtime.cpp','-o',runtime],targets[1])
                run([clangxx,*cppcommon,'-Dmorrow_run=mp_guest_run',EXT/'guests/cpp/plugin.cpp',runtime,*objects,library,*link,'-Wl,--export=__wasm_call_ctors','-L'+str(stdlib/'noeh'),'-L'+str(stdlib),'-lc++','-lc++abi','-lc','-o',module],targets[1])
            if not 8 <= module.stat().st_size <= 4*1024*1024 or module.read_bytes()[:8]!=b'\0asm\x01\0\0\0':
                raise ValueError('invalid Wasm output')
            package=destination/'plugin.mplugin'
            run([pack,'pack',module,package,'org.morrow.changes.'+language,'0.1.0','changes-metadata-v1','1',PROFILE],targets[1])
            manifest['artifacts'][language]={kind:{'path':str(file),'sha256':sha(file),'bytes':file.stat().st_size} for kind,file in [('wasm',module),('package',package)]};save()
        if before!={str(x.relative_to(ROOT)):sha(x) for x in source_paths} or transport!={str(x.relative_to(ROOT)):sha(x) for x in sorted((ROOT/'sdk').rglob('*')) if x.is_file()}:
            raise ValueError('source bytes changed during build')
        validate_contract();manifest['status']='built_not_runtime_qualified';save()
        print(json.dumps(manifest['artifacts'],indent=2))
    except Exception as e:
        manifest['status']='failed';manifest['failure']=str(e);save();raise

if __name__=='__main__':
    main()
