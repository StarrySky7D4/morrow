"""Local offline qualification, disposable stores, raw logs and before/after pins."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time

PACKAGE = Path(__file__).resolve().parents[1]
SOURCE = PACKAGE.parents[1]
RUNS = SOURCE.parent / "wasm-client-runs"
parser = argparse.ArgumentParser()
parser.add_argument("mode", choices=["lock", "client", "guest", "qualification", "format", "combined-guest"])
parser.add_argument("label")
parser.add_argument("--rust-bin", type=Path, required=True, help="Explicit Rust toolchain bin directory; no machine-specific default.")
parser.add_argument("--vs-dev-cmd", type=Path, required=True, help="Explicit reviewed VsDevCmd.bat path.")
parser.add_argument("--capnp-bin", type=Path, required=True, help="Explicit Capnp compiler directory.")
args = parser.parse_args()
# Public portable configuration; historical fixed-machine receipts do not qualify this invocation.
RUST = args.rust_bin.resolve(strict=True)
VS_DEV_CMD = args.vs_dev_cmd.resolve(strict=True)
CAPNP_BIN = args.capnp_bin.resolve(strict=True)
if os.name != "nt" or not RUST.is_dir() or not CAPNP_BIN.is_dir() or not VS_DEV_CMD.is_file():
    raise ValueError("Windows and explicit existing toolchain paths are required")
if any(c in str(VS_DEV_CMD) for c in '\r\n"&|<>^%!'):
    raise ValueError("VsDevCmd path contains command metacharacters")
CARGO = RUST / "cargo.exe"
for tool in (CARGO, RUST / "rustc.exe", RUST / "rustdoc.exe", CAPNP_BIN / "capnp.exe"):
    if not tool.is_file():
        raise ValueError("Required explicit compiler tool is absent")

if not args.label.replace("-", "").isalnum(): raise ValueError("invalid label")
OUT = RUNS / args.label
OUT.mkdir(parents=True, exist_ok=False)
env = os.environ.copy()
for name in ("RUSTC", "RUSTDOC", "CC", "CXX", "AR", "CARGO_TARGET_DIR"):
    env.pop(name, None)
dev = str(VS_DEV_CMD)
setup = subprocess.run(f'cmd.exe /d /s /c ""{dev}" -no_logo -arch=amd64 >nul && set"', capture_output=True, text=True, check=True)
for line in setup.stdout.splitlines():
    if "=" in line and not line.startswith("="):
        key, value = line.split("=", 1); env[key] = value
env["PATH"] = str(RUST) + ";" + str(CAPNP_BIN) + ";" + env["PATH"]
env["RUSTC"] = str(RUST / "rustc.exe")
env["RUSTDOC"] = str(RUST / "rustdoc.exe")
# Use the compiler discovered in this explicit VS environment, not an old local installation.
env["CC"] = shutil.which("cl.exe", path=env["PATH"]) or ""
if not env["CC"]:
    raise ValueError("MSVC cl.exe is absent from the selected VS environment")
env["CARGO_BUILD_JOBS"] = "4"
env["PYTHONUTF8"] = "1"
for key, name in (("TMP", "tmp"), ("LOCALAPPDATA", "local-app-data")):
    location = OUT / name; location.mkdir(); env[key] = str(location)
env["TEMP"] = env["TMP"]
# Separate target for this component; reused only by its own staged qualification.
TARGET = RUNS / "target-native"
WASM_TARGET = RUNS / ("wc" if args.mode == "combined-guest" else "target-wasm")
env["MORROW_CODEX_R2_GUEST"] = str(WASM_TARGET / "wasm32-unknown-unknown/release/morrow_codex_session_exec_guest_r2.wasm")

def sha(raw): return hashlib.sha256(raw).hexdigest()
def pins():
    directories = [PACKAGE, SOURCE / "core", SOURCE / "extensions/agent-session-exec-v1-r2", SOURCE / "sdk/rust"]
    if args.mode in ("qualification", "combined-guest"): directories.append(SOURCE / "plugin_runtime")
    result = {}
    for directory in directories:
        for p in directory.rglob("*"):
            if p.is_file() and p.suffix in (".rs", ".toml", ".lock", ".proto", ".capnp", ".py", ".md", ".txt") and not any(x in ("target", "__pycache__") for x in p.parts):
                result[p.relative_to(SOURCE).as_posix()] = sha(p.read_bytes())
    return dict(sorted(result.items()))
before = pins()
(OUT / "source-before.json").write_text(json.dumps(before, indent=2), encoding="utf-8")
commands = []
if args.mode == "lock":
    commands = [[CARGO, "generate-lockfile", "--offline", "--manifest-path", p / "Cargo.toml"] for p in (PACKAGE, PACKAGE / "guest", PACKAGE / "qualification")]
elif args.mode == "client":
    commands = [[CARGO,"test","--locked","--offline","--manifest-path",PACKAGE / "Cargo.toml","--target-dir",TARGET,"--test","client","--","--nocapture"]]
elif args.mode == "guest" or args.mode == "combined-guest":
    commands = [[CARGO,"build","--locked","--offline","--manifest-path",PACKAGE / "guest/Cargo.toml","--target","wasm32-unknown-unknown","--release","--target-dir",WASM_TARGET]]
    if args.mode == "combined-guest": commands[0].extend(["--features", "process-profile"])
elif args.mode == "qualification":
    commands = [[CARGO,"test","--locked","--offline","--manifest-path",PACKAGE / "qualification/Cargo.toml","--target-dir",TARGET,"--test","real_guest","--","--nocapture"]]
elif args.mode == "format":
    commands = [[CARGO,"fmt","--manifest-path",p / "Cargo.toml"] for p in (PACKAGE,PACKAGE / "guest",PACKAGE / "qualification")]
records = []
for n, command in enumerate(commands):
    command = [str(x) for x in command]
    started = time.time()
    with (OUT / f"{n}-stdout.log").open("wb") as stdout, (OUT / f"{n}-stderr.log").open("wb") as stderr:
        run = subprocess.run(command,cwd=SOURCE,env=env,stdout=stdout,stderr=stderr)
    raw, errors = (OUT / f"{n}-stdout.log").read_bytes(), (OUT / f"{n}-stderr.log").read_bytes()
    record = {"args":command,"exit_code":run.returncode,"seconds":round(time.time()-started,2),"stdout_sha256":sha(raw),"stderr_sha256":sha(errors)}
    records.append(record)
    print(json.dumps(record),flush=True)
    print((raw + errors).decode("utf-8",errors="replace")[-5000:],flush=True)
    if run.returncode: break
after = pins()
(OUT / "source-after.json").write_text(json.dumps(after,indent=2),encoding="utf-8")
changed = [key for key in sorted(set(before) | set(after)) if before.get(key) != after.get(key)]
artifact = Path(env["MORROW_CODEX_R2_GUEST"])
summary = {"mode":args.mode,"records":records,"source_before_sha256":sha(json.dumps(before,sort_keys=True).encode()),"source_after_sha256":sha(json.dumps(after,sort_keys=True).encode()),"changed_paths":changed,"wasm_sha256":sha(artifact.read_bytes()) if artifact.is_file() else None,"wasm_path":str(artifact)}
(OUT / "receipt.json").write_text(json.dumps(summary,indent=2),encoding="utf-8")
print(json.dumps(summary),flush=True)
raise SystemExit(0 if all(r["exit_code"] == 0 for r in records) and (not changed or args.mode in ("lock", "format")) else 1)
