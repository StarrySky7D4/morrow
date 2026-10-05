#!/usr/bin/env python3
"""Build NEW typed SSE guests offline in an external evidence directory.

Only explicitly supplied external Cargo caches are reused. Never rebuild/repack
old guest artifacts or mutate old SDK distributions, contracts, pins or locks.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tomllib

PROFILE = 'bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863'
ROOT = Path(__file__).resolve().parents[1]
EXT = ROOT / 'extensions/sse-event-v1'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reject_reparse(path):
    supplied = Path(os.path.abspath(path))
    for ancestor in [supplied, *supplied.parents]:
        try:
            metadata = os.lstat(ancestor)
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & 0x400:
            raise ValueError('symlink or reparse input/output ancestor is forbidden')
    return supplied


def external(path):
    value = reject_reparse(path).resolve()
    if value == ROOT or ROOT in value.parents:
        raise ValueError('outputs and target caches must be external to the source repository')
    return value


def sysroot_snapshot(path):
    root = reject_reparse(path).resolve(strict=True)
    result = {}
    for directory, directories, files in os.walk(root, followlinks=False):
        for name in [*directories, *files]:
            selected = Path(directory)/name
            metadata = os.lstat(selected)
            if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & 0x400:
                raise ValueError('symlink or reparse sysroot input is forbidden')
            if name in files:
                if not stat.S_ISREG(metadata.st_mode):
                    raise ValueError('sysroot inputs must be regular files')
                result[str(selected.relative_to(root)).replace('\\','/')] = sha(selected)
    if not result:
        raise ValueError('empty sysroot input set')
    return result


def validate_contract():
    core = ROOT / 'network_node_stream_001/schemas/sse_event.capnp'
    sdk = EXT / 'contracts/sse_event.capnp'
    if core.read_bytes() != sdk.read_bytes() or sha(sdk) != PROFILE:
        raise ValueError('Native/extension contract bytes or frozen profile digest mismatch')


def validate_dependencies():
    original = {(p['name'], p['version'], p.get('source')): p.get('checksum')
                for p in tomllib.loads((ROOT/'sdk/rust/Cargo.lock').read_text())['package'] if 'source' in p}
    for dependency in tomllib.loads((EXT/'Cargo.lock').read_text())['package']:
        if 'source' in dependency:
            key = (dependency['name'], dependency['version'], dependency['source'])
            if key not in original or original[key] != dependency.get('checksum'):
                raise ValueError('extension dependency does not match original exact SDK lock: '+dependency['name'])


def disjoint_paths(output, targets):
    paths = [output, *targets]
    for i, left in enumerate(paths):
        for right in paths[i+1:]:
            if left == right or left in right.parents or right in left.parents:
                raise ValueError('output and dedicated target paths must be disjoint')


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
    cc, cxx, root = (reject_reparse(x).resolve(strict=True) for x in (cc, cxx, root))
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
    validate_dependencies()
    output = external(args.output)
    targets = [external(args.rust_target), external(args.c_target)]
    if output.exists():
        raise ValueError('output must not already exist; refusing overwrite')
    wasi = Path(args.wasi_sdk).resolve(strict=True)
    pack = Path(args.pack_executable).resolve(strict=True)
    clang, clangxx, sysroot = compiler_paths(wasi, args.clang, args.clangxx, args.sysroot)
    disjoint_paths(output, targets)
    sysroot_before = sysroot_snapshot(sysroot)
    source_paths = sorted(x for x in EXT.rglob('*') if x.is_file() and 'target' not in x.parts)
    source_paths += [Path(__file__).resolve(), ROOT / 'network_node_stream_001/examples/sse_sdk_package.rs', ROOT/'network_node_stream_001/schemas/sse_event.capnp']
    before = {str(x.relative_to(ROOT)): sha(x) for x in source_paths}
    # Include exact original transport source bytes; copying a compiler cache is
    # not a claim that any historical guest has been requalified.
    transport = {str(x.relative_to(ROOT)): sha(x) for x in sorted((ROOT/'sdk').rglob('*')) if x.is_file()}
    output.mkdir(parents=True, exist_ok=False)
    manifest = {'profile': PROFILE, 'sources': before, 'transport_sources': transport,
                'pack_executable': {'path': str(pack), 'sha256': sha(pack)},
                'targets': list(map(str,targets)), 'commands': [], 'artifacts': {}, 'status': 'building', 'network_fallback': False, 'sysroot_inputs_before': sysroot_before}
    def save():
        (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    def run(command, target):
        command = list(map(str,command))
        env = {k:v for k,v in os.environ.items() if not k.upper().startswith('MORROW_')}
        env.update(CARGO_TARGET_DIR=str(target), CARGO_NET_OFFLINE='true')
        number = len(manifest['commands']) + 1
        record = {'argv': command, 'stdout': f'command-{number:03}-stdout.raw', 'stderr': f'command-{number:03}-stderr.raw'}
        manifest['commands'].append(record); save()
        with (output/record['stdout']).open('wb') as out, (output/record['stderr']).open('wb') as err:
            process = subprocess.run(command, cwd=ROOT, env=env, stdout=out, stderr=err)
        record['raw_exit'] = process.returncode
        record['stdout_sha256'] = sha(output/record['stdout']); record['stderr_sha256'] = sha(output/record['stderr']); save()
        process.check_returncode()
    try:
        toolchains = {}
        for name, command in [('rustc',['rustc','--version']),('cargo',['cargo','--version']),('capnp',['capnp','--version']),('clang',[clang,'--version']),('clangxx',[clangxx,'--version']),('wasmld',[clang.parent/('wasm-ld.exe' if os.name=='nt' else 'wasm-ld'),'--version'])]:
            executable=Path(shutil.which(str(command[0])) or command[0]).resolve(strict=True)
            toolchains[name] = {'version':subprocess.check_output(list(map(str,command)),text=True).strip(),'path':str(executable),'sha256':sha(executable)}
        manifest['toolchains']=toolchains; save()
        if not toolchains['rustc']['version'].startswith('rustc 1.95.0 ') or toolchains['capnp']['version'] != "Cap'n Proto version 1.4.0":
            raise ValueError('exact Rust 1.95.0 and capnp 1.4.0 are required')
        manifest['toolchains']=toolchains; save()
        cargo=['cargo','build','--locked','--offline','--manifest-path',EXT/'Cargo.toml','--target','wasm32-unknown-unknown','--release']
        run(cargo+['-p','morrow-sse-event-guest','--target-dir',targets[0]],targets[0])
        rust_wasm=targets[0]/'wasm32-unknown-unknown/release/morrow_sse_event_guest.wasm'
        run(cargo+['-p','morrow-sse-event-v1','--features','c-transport','--target-dir',targets[1]],targets[1])
        library=targets[1]/'wasm32-unknown-unknown/release/libmorrow_sse_event_v1.a'
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
            run([pack,'pack',module,package,'org.morrow.sse.sdk.'+language,'0.1.0','sse-event-v1','1',PROFILE],targets[1])
            manifest['artifacts'][language]={kind:{'path':str(file),'sha256':sha(file),'bytes':file.stat().st_size} for kind,file in [('wasm',module),('package',package)]};save()
        if before!={str(x.relative_to(ROOT)):sha(x) for x in source_paths} or transport!={str(x.relative_to(ROOT)):sha(x) for x in sorted((ROOT/'sdk').rglob('*')) if x.is_file()}:
            raise ValueError('source bytes changed during build')
        validate_contract();validate_dependencies()
        manifest['generated_schemas']={str(p):sha(p) for target in targets for p in target.glob('wasm32-unknown-unknown/release/build/*/out/*') if p.is_file()}
        manifest['source_after']={str(x.relative_to(ROOT)):sha(x) for x in source_paths}
        manifest['transport_sources_after']={str(x.relative_to(ROOT)):sha(x) for x in sorted((ROOT/'sdk').rglob('*')) if x.is_file()}
        manifest['sysroot_inputs_after']=sysroot_snapshot(sysroot)
        if manifest['sysroot_inputs_after']!=sysroot_before:
            raise ValueError('selected sysroot header/library bytes changed during preparation')
        manifest['toolchains_after']={name:{**properties,'sha256':sha(Path(properties['path']))} for name,properties in toolchains.items()}
        manifest['pack_executable_after']={'path':str(pack),'sha256':sha(pack)}
        if manifest['toolchains_after']!=toolchains or manifest['pack_executable_after']!=manifest['pack_executable']:
            raise ValueError('toolchain or selected packer changed during preparation')
        manifest['status']='built_not_runtime_qualified';save()
        print(json.dumps(manifest['artifacts'],indent=2))
    except Exception as e:
        manifest['status']='failed';manifest['failure']=str(e)
        manifest['source_after']={str(x.relative_to(ROOT)):sha(x) for x in source_paths};save();raise

if __name__=='__main__':
    main()
