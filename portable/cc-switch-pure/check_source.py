"""Read-only pinned source parity; optional actual CCSwitch checkout argument."""
from pathlib import Path
import argparse, hashlib, json

def sha(data):
    return hashlib.sha256(data).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, help="Original cc-switch-source root")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    receipt = json.loads((root / "source-receipt.json").read_text(encoding="utf-8"))
    module = (root / "src/model_capabilities.rs").read_bytes()
    constant = (root / "src/claude_desktop_config.rs").read_bytes().removesuffix(b"\n")
    assert sha(module) == receipt["original_module_sha256"], "copied module changed"
    assert sha(constant) == receipt["constant_definition_utf8_sha256"], "extracted definition changed"
    if args.upstream:
        actual = args.upstream / "src-tauri/src"
        assert (actual / "model_capabilities.rs").read_bytes() == module, "upstream module mismatch"
        config = (actual / "claude_desktop_config.rs").read_bytes()
        assert sha(config) == receipt["original_config_sha256"], "upstream context source changed"
        assert config.splitlines()[receipt["constant_line"] - 1] == constant, "definition extraction mismatch"
        assert sha((args.upstream / "src-tauri/Cargo.lock").read_bytes()) == receipt["original_lock_sha256"], "upstream lock changed"
    print(json.dumps({"module_parity": True, "constant_parity": True,
                      "actual_upstream_checked": bool(args.upstream),
                      "scope": "source parity only, not execution or G0 qualification"}))

if __name__ == "__main__":
    main()
