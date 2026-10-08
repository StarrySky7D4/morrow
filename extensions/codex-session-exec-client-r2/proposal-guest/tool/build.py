"""Build only this independent Rust proposal guest; record logs and source pins."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,time
PACKAGE=Path(__file__).resolve().parents[1]
CLIENT=PACKAGE.parent
SOURCE=CLIENT.parents[1]
RUNS=SOURCE.parent/"proposal-guest-runs"
parser=argparse.ArgumentParser()
parser.add_argument("mode",choices=["lock","format","build"])
parser.add_argument("label")
parser.add_argument("--rust-bin", type=Path, required=True, help="Explicit Rust toolchain bin directory; no machine-specific default.")
parser.add_argument("--vs-dev-cmd", type=Path, required=True, help="Explicit reviewed VsDevCmd.bat path.")
parser.add_argument("--capnp-bin", type=Path, required=True, help="Explicit Capnp compiler directory.")
args=parser.parse_args()
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

if not args.label.replace("-","").isalnum():raise ValueError("invalid label")
OUT=RUNS/args.label
OUT.mkdir(parents=True,exist_ok=False)
env=os.environ.copy()
for key in ("RUSTC","RUSTDOC","CC","CXX","AR","CARGO_TARGET_DIR"):env.pop(key,None)
dev = str(VS_DEV_CMD)
setup=subprocess.run(f'cmd.exe /d /s /c ""{dev}" -no_logo -arch=amd64 >nul && set"',capture_output=True,text=True,check=True)
for line in setup.stdout.splitlines():
    if "=" in line and not line.startswith("="):
        key,value=line.split("=",1);env[key]=value
env["PATH"] = str(RUST) + ";" + str(CAPNP_BIN) + ";" + env["PATH"]
env["RUSTC"]=str(RUST/"rustc.exe")
env["RUSTDOC"]=str(RUST/"rustdoc.exe")
env["CARGO_BUILD_JOBS"]="4"
for key,name in (("TMP","tmp"),("LOCALAPPDATA","local-app-data")):
    path=OUT/name;path.mkdir();env[key]=str(path)
env["TEMP"]=env["TMP"]
TARGET=RUNS/"pg"
ARTIFACT=TARGET/"wasm32-unknown-unknown/release/morrow_codex_proposal_guest_r2.wasm"
def sha(raw):return hashlib.sha256(raw).hexdigest()
def pins():
    result={}
    for directory in (CLIENT,SOURCE/"core",SOURCE/"extensions/agent-session-exec-v1-r2",SOURCE/"sdk/rust"):
        for p in directory.rglob("*"):
            if p.is_file() and p.suffix in (".rs",".toml",".lock",".capnp",".proto",".py",".md",".txt") and not any(x in ("target","__pycache__") for x in p.parts):
                result[p.relative_to(SOURCE).as_posix()]=sha(p.read_bytes())
    return dict(sorted(result.items()))
before=pins()
(OUT/"source-before.json").write_text(json.dumps(before,indent=2),encoding="utf-8")
if args.mode=="lock":command=[CARGO,"generate-lockfile","--offline","--manifest-path",PACKAGE/"Cargo.toml"]
elif args.mode=="format":command=[CARGO,"fmt","--manifest-path",PACKAGE/"Cargo.toml"]
else:command=[CARGO,"build","--locked","--offline","--manifest-path",PACKAGE/"Cargo.toml","--target","wasm32-unknown-unknown","--release","--target-dir",TARGET]
command=[str(x) for x in command];started=time.time()
with (OUT/"stdout.log").open("wb") as stdout,(OUT/"stderr.log").open("wb") as stderr:
    run=subprocess.run(command,cwd=SOURCE,env=env,stdout=stdout,stderr=stderr)
raw,errors=(OUT/"stdout.log").read_bytes(),(OUT/"stderr.log").read_bytes()
after=pins()
(OUT/"source-after.json").write_text(json.dumps(after,indent=2),encoding="utf-8")
changed=[key for key in sorted(set(before)|set(after)) if before.get(key)!=after.get(key)]
record={"mode":args.mode,"args":command,"exit_code":run.returncode,"seconds":round(time.time()-started,2),"stdout_sha256":sha(raw),"stderr_sha256":sha(errors),"source_before_sha256":sha(json.dumps(before,sort_keys=True).encode()),"source_after_sha256":sha(json.dumps(after,sort_keys=True).encode()),"changed_paths":changed,"wasm_path":str(ARTIFACT),"wasm_sha256":sha(ARTIFACT.read_bytes()) if ARTIFACT.is_file() else None}
(OUT/"receipt.json").write_text(json.dumps(record,indent=2),encoding="utf-8")
print((raw+errors).decode("utf-8",errors="replace")[-4500:])
print(json.dumps(record),flush=True)
raise SystemExit(0 if run.returncode==0 and (not changed or args.mode in ("lock","format")) else 1)
