"""Attach final kit and build-message identity checks to the independent review."""
from pathlib import Path
import hashlib, json
HERE=Path(__file__).resolve().parent
RUN=HERE/'runs/p02-native-probe-001-review-004'
PLUGIN=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    result=read(RUN/'result.json')
    assert result['status']=='verified_limited'
    kit=PLUGIN/'sdk/host-kit-003-copy'; manifest=read(kit/'manifest.json')
    assert sha(kit/'manifest.json')=='5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01'
    files={f['path']:sha(kit/f['path']) for f in manifest['files']}
    source={f['path']:sha(Path(manifest['authority'])/f['path']) for f in manifest['source_files']}
    assert files=={f['path']:f['sha256'] for f in manifest['files']}
    assert source=={f['path']:f['sha256'] for f in manifest['source_files']}
    before=read(HERE/'runs/p02-native-probe-001-review-003/kit-copy-during-build.json')
    assert before['files']==files and before['authority_source_files']==source
    assert {p.relative_to(kit).as_posix() for p in kit.rglob('*') if p.is_file()}==set(files)|{'manifest.json'}
    assert 'release: 1.95.0' in (RUN/'rustc.stdout').read_text(encoding='utf-8')
    assert 'host: x86_64-pc-windows-msvc' in (RUN/'rustc.stdout').read_text(encoding='utf-8')
    assert 'cargo 1.95.0 ' in (RUN/'cargo.stdout').read_text(encoding='utf-8')
    messages=[json.loads(line) for line in (RUN/'build.stdout').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
    assert messages[-1]=={'reason':'build-finished','success':True}
    artifacts={m['package_id'] for m in messages if m['reason']=='compiler-artifact'}
    scripts=[m['package_id'] for m in messages if m['reason']=='build-script-executed']
    executable=Path(result['independent_consumer']['executable'])
    assert any(m['reason']=='compiler-artifact' and m.get('executable') and Path(m['executable'])==executable for m in messages)
    evidence={'status':'verified','kit_files':180,'authority_source_files':12,'kit_unchanged_across_independent_build':True,
      'kit_prebuild_evidence':'../p02-native-probe-001-review-003/kit-copy-during-build.json',
      'compiler_artifact_package_ids':len(artifacts),'build_script_messages':len(scripts),'build_script_unique_packages':len(set(scripts)),
      'toolchain':'Rust/Cargo 1.95.0 x86_64-pc-windows-msvc','old_tests_repeated':False,
      'relocation':read(RUN/'consumer-relocation.json'),
      'executable_difference_limit':'Consumer CARGO_MANIFEST_DIR/output guard and build paths differ; executable byte reproducibility is not claimed.',
      'build_output_scope':str(HERE/'t-p02-004'),'kit_manifest_sha256':sha(kit/'manifest.json')}
    (RUN/'final-identity.json').write_text(json.dumps(evidence,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(evidence,ensure_ascii=False))
if __name__=='__main__': main()
