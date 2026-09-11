"""Mirror fixed host contracts into the independently buildable guest SDK."""
import argparse
from pathlib import Path
import re

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true", help="fail if snapshots differ")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
source = (root / "core/src/runtime.rs").read_text(encoding="utf-8")
match = re.search(r"PROTOCOL_VERSION: u16 = (\d+)", source)
if match is None:
    raise SystemExit("Host protocol version not found")
files = {
    name: (root / "core/schemas" / name).read_text(encoding="utf-8")
    for name in ("runtime.capnp", "content.proto", "task.capnp")
}
files["version.txt"] = match.group(1) + "\n"
for name, body in files.items():
    target = root / "sdk/rust/contracts" / name
    if args.check:
        if not target.exists() or target.read_text(encoding="utf-8") != body:
            raise SystemExit("Stale guest SDK contract: " + name)
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body, encoding="utf-8", newline="\n")
print("Verified guest SDK contracts." if args.check else "Updated guest SDK contract snapshots.")
