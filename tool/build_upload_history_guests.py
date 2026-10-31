"""Build fresh task-bound history consumers and fixtures, with only task imports."""
from pathlib import Path
import argparse, json, os, shutil, subprocess, sys, time
import windows_qualification_build_cache as cache
import verify_structural_repairs as source
import build_directory_guest_samples as wasm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--sysroot', required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    permitted = Path('D:/build_cache').resolve(strict=True)
    if not output.is_relative_to(permitted) or output == permitted or output.exists():
        raise ValueError('Use a new output directory beneath D:/build_cache')
    for parent in output.parents:
        if parent.exists() and (parent.is_junction() or parent.is_symlink()):
            raise ValueError('Build output ancestors must not be reparse points')
    sysroot = args.sysroot.resolve(strict=True)
    toolchain = Path('C:/Users/Administrator/.rustup/toolchains/1.96.0-x86_64-pc-windows-msvc/bin')
    llvm = Path('C:/Program Files/LLVM/bin')
    tools = [toolchain / n for n in ('cargo.exe', 'rustc.exe')]
    tools += [llvm / n for n in ('clang.exe', 'clang++.exe', 'wasm-ld.exe')]
    tools += [Path('C:/Users/Administrator/capnp-bin/capnp.exe')]
    libraries = sysroot / 'lib/wasm32-wasip1'
    sysroot_files = [p for p in (sysroot / 'include').rglob('*') if p.is_file()]
    sysroot_files += [libraries / p for p in ('libc.a', 'noeh/libc++.a', 'noeh/libc++abi.a')]
    input_files = [p for p in (repo / 'extensions/blob-upload-history-v1').rglob('*') if p.is_file()]
    for root in ('sdk/rust', 'sdk/c/include', 'sdk/cpp/include'):
        input_files += [p for p in (repo / root).rglob('*') if p.is_file()]
    input_files += [repo / p for p in ('tool/build_upload_history_guests.py', 'tool/build_directory_guest_samples.py',
        'sdk/c/src/morrow_plugin_wasm_libc.c', 'sdk/c/src/morrow_plugin_task.c', 'sdk/cpp/src/morrow_plugin_wasm_runtime.cpp')]
    source_pins = {str(p): cache.pin(p) for p in input_files}
    sysroot_pins = {str(p): cache.pin(p) for p in sysroot_files}
    tool_pins = {str(p): cache.pin(p) for p in tools}
    before = source.source_digest()
    output.mkdir(parents=True)

    def save(name, data):
        with (output / name).open('x', encoding='utf-8') as stream:
            json.dump(data, stream, indent=2)

    save('inputs.json', dict(source_digest=before, source_files=source_pins, sysroot_files=sysroot_pins,
        tools=tool_pins, rust_target='wasm32-unknown-unknown', clang_target='wasm32-wasip1',
        separate_empty_targets=['rust-target', 'ffi-target'], no_WASI_imports_allowed=True))
    env = {k:v for k,v in cache.environment(output / 'tmp').items() if not k.startswith('MORROW_')}
    for k in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR'):
        env.pop(k, None)
    env.update(PATH=str(toolchain)+';C:/Users/Administrator/capnp-bin;'+env['PATH'], RUSTUP_TOOLCHAIN='1.96.0',
        RUSTC=str(toolchain/'rustc.exe'), CAPNP='C:/Users/Administrator/capnp-bin/capnp.exe',
        PYTHONDONTWRITEBYTECODE='1', PYTHONUTF8='1', CARGO_PROFILE_RELEASE_DEBUG='0')
    commands = []

    def run(name, command):
        save(name+'-command.json', dict(argv=list(map(str, command)), cwd=str(repo)))
        start = time.monotonic()
        with (output/(name+'.stdout')).open('xb') as out, (output/(name+'.stderr')).open('xb') as err:
            process = subprocess.Popen(list(map(str, command)), cwd=repo, env=env, stdout=out, stderr=err, creationflags=subprocess.CREATE_NO_WINDOW)
            save(name+'-process.json', dict(pid=process.pid, original_handle_held=True))
            code = process.wait()
        row = dict(name=name, exit_code=code, original_process_returned=True, seconds=round(time.monotonic()-start, 3),
            stdout=cache.pin(output/(name+'.stdout')), stderr=cache.pin(output/(name+'.stderr')))
        commands.append(row);save(name+'-result.json', row)
        print(json.dumps(dict(phase=name, exit_code=code)), flush=True)
        if code:
            raise RuntimeError(name+' failed; no automatic replay')
        return (output/(name+'.stdout')).read_text(encoding='utf-8')

    error = None;artifacts = {};compiler = {}
    try:
        for name, manifest, target in [('rust', 'guests/rust/Cargo.toml', 'rust-target'), ('ffi', 'guests/ffi-support/Cargo.toml', 'ffi-target')]:
            text = run(name+'-build', [tools[0], 'build', '--locked', '--offline', '--release', '--jobs', '1', '--manifest-path',
                repo/'extensions/blob-upload-history-v1'/manifest, '--target', 'wasm32-unknown-unknown', '--target-dir', output/target, '--message-format=json'])
            records = [json.loads(line) for line in text.splitlines()]
            assert [r['success'] for r in records if r.get('reason')=='build-finished']==[True]
            records = [r for r in records if r.get('reason')=='compiler-artifact']
            assert records and all(not r['fresh'] for r in records)
            compiler[name] = records
        shutil.copyfile(output/'rust-target/wasm32-unknown-unknown/release/morrow_history_fixture_guest.wasm', output/'rust_history.wasm')
        common = ['--target=wasm32-wasip1', '--sysroot='+str(sysroot), '-O2', '-Wall', '-Wextra', '-Werror',
            '-Isdk/c/include', '-Iextensions/blob-upload-history-v1/include']
        objects = []
        for name in ('morrow_plugin_wasm_libc', 'morrow_plugin_task'):
            obj = output/(name+'.o');objects.append(obj)
            run(name, [llvm/'clang.exe', *common, '-std=c11', '-c', repo/'sdk/c/src'/(name+'.c'), '-o', obj])
        codec = output/'ffi-target/wasm32-unknown-unknown/release/libmorrow_history_fixture_codecs.a'
        link = ['-nostdlib', '-Wl,--no-entry', '-Wl,--export=morrow_run', '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=16777216', '-Wl,--strip-all']
        run('c-link', [llvm/'clang.exe', *common, '-std=c11', repo/'extensions/blob-upload-history-v1/guests/c/plugin.c', *objects, codec,
            *link, '-L'+str(libraries), '-lc', '-o', output/'c_history.wasm'])
        cpp = [*common, '-Isdk/cpp/include', '-std=c++17', '-nostdinc++', '-isystem', sysroot/'include/wasm32-wasip1/noeh/c++/v1', '-fno-exceptions', '-fno-rtti']
        runtime = output/'morrow_plugin_wasm_runtime.o'
        run('cpp-runtime', [llvm/'clang++.exe', *cpp, '-c', repo/'sdk/cpp/src/morrow_plugin_wasm_runtime.cpp', '-o', runtime])
        run('cpp-link', [llvm/'clang++.exe', *cpp, '-Dmorrow_run=mp_guest_run', repo/'extensions/blob-upload-history-v1/guests/cpp/plugin.cpp', runtime,
            *objects, codec, *link, '-Wl,--export=__wasm_call_ctors', '-L'+str(libraries/'noeh'), '-L'+str(libraries), '-lc++', '-lc++abi', '-lc', '-o', output/'cpp_history.wasm'])
        wasm.EXPECTED_IMPORTS = {('morrow_task_v1','read_input'):([0x7f,0x7f],[0x7f]), ('morrow_task_v1','complete'):([0x7f,0x7f],[0x7f])}
        artifacts = {name:dict(pin=cache.pin(output/(name+'_history.wasm')), interface=wasm.inspect_wasm(output/(name+'_history.wasm'))) for name in ('rust','c','cpp')}
    except Exception as exc:
        error = type(exc).__name__+': '+str(exc)
    stable = source.source_digest()==before and all(cache.pin(Path(p))==v for p,v in source_pins.items())
    tools_stable = all(cache.pin(Path(p))==v for p,v in {**tool_pins, **sysroot_pins}.items())
    passed = error is None and stable and tools_stable
    save('compiler-proof.json', compiler)
    save('result.json', dict(status='FRESH_HISTORY_WASM_COMPILED_NOT_RUNTIME' if passed else 'FAILURE_RETAINED_NOT_ACCEPTED',
        source_digest=before, source_unchanged=stable, toolchain_and_sysroot_unchanged=tools_stable,
        commands=commands, all_original_processes_returned=True, artifacts=artifacts, error=error,
        cache_root=str(permitted), runtime='NOT_RUN', public_product_delivery=False, source_accepted=False, release_eligible=False))
    print(json.dumps(dict(passed=passed, error=error)), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
