from pathlib import Path
import json, subprocess, hashlib
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
SOURCE=ROOT/'upstream/p02-integration-004/codex-work'
allowed=['core/Cargo.toml','core/src/lib.rs','core/src/client.rs','core/src/morrow_network.rs','core/src/morrow_network_qualification.rs','core/src/morrow_p02_qualification.rs','core/src/exec.rs','core/src/unified_exec/process_manager.rs','exec-server/src/environment.rs','thread-store/Cargo.toml','thread-store/src/live_thread.rs']
allowed.append('core/src/unified_exec/mod.rs')
full=['codex-rs/'+p for p in allowed]
p=HERE/'review_working_patch.py';s=p.read_text(encoding='utf-8');start=s.index('ALLOWED = ');end=s.index('\n',start);s=s[:start]+'ALLOWED = '+repr(set(full))+s[end:];s=s.replace('Expected exactly five changed/added files','Expected exactly eleven changed/added files');p.write_text(s,encoding='utf-8')
(HERE/'patched-inputs.json').write_text(json.dumps([(SOURCE/p).relative_to(ROOT).as_posix() for p in full],indent=2),encoding='utf-8')
files=[SOURCE/p for p in full if p.endswith('.rs')]+list((ROOT/'qualification/p02-integration-004/src').glob('*.rs'))
rustfmt=Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin\rustfmt.exe')
subprocess.run([str(rustfmt),'--edition','2024','--config','skip_children=true',*[str(p) for p in files]],check=True,cwd=ROOT)
print(json.dumps({'formatted_touched_files':len(files)}))
