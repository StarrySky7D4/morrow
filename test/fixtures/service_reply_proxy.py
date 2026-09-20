# Test-only proxy; never used by the shipped application.
#!/usr/bin/env python3
import hashlib, json, struct, subprocess, sys
from pathlib import Path

MAX_REQ, MAX_REP = 131072, 262144

def die(msg):
    print(msg, file=sys.stderr)
    sys.exit(2)

def read_exact(f, n):
    parts, left = [], n
    while left:
        b = f.read(left)
        if b == b"":
            raise EOFError("unexpected EOF mid-frame")
        parts.append(b)
        left -= len(b)
    return b"".join(parts)

def log_trace(path, event):
    with open(path, "a", encoding="utf-8") as t:
        t.write(json.dumps(event) + "\n")

def matches_request(root, req):
    armed = root / "armed.sha256"
    if armed.exists():
        return hashlib.sha256(req).hexdigest() == armed.read_text(encoding="ascii").strip()
    mask_file = root / "armed.mask.json"
    if not mask_file.exists():
        return False
    if len(req) < 1 or len(req) > MAX_REQ:
        raise ValueError("invalid request length")
    with mask_file.open("rb") as source:
        raw = source.read(4 * MAX_REQ + 129)
    if len(raw) > 4 * MAX_REQ + 128:
        raise ValueError("mask config file too large")
    cfg = json.loads(raw.decode("ascii"))
    template = bytes.fromhex(cfg["template"])
    mask = bytes.fromhex(cfg["mask"])
    if not 1 <= len(template) <= MAX_REQ or len(mask) != len(template):
        raise ValueError("invalid template or mask length")
    if not all(m in (0, 255) for m in mask):
        raise ValueError("mask bytes must be 0 or 255 only")
    if not any(m == 255 for m in mask):
        raise ValueError("mask must cover at least one byte")
    if len(req) != len(template):
        return False
    return all((a & m) == (b & m) for a, b, m in zip(req, template, mask))

def main():
    root = Path(__file__).resolve().parent
    cfg = json.loads((root / "proxy.json").read_text(encoding="utf-8"))
    host, store, package = Path(cfg["host"]), Path(cfg["store"]), Path(cfg["package"])
    mode = cfg["mode"]
    if mode not in ("malformed", "eof"):
        die("invalid mode")
    trace = root / "trace.jsonl"
    receipt = root / "receipt.bin"

    child = subprocess.Popen(
        [str(host), "--managed", str(store), str(package)],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=None, shell=False,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    cin, cout = child.stdin, child.stdout
    injected = False
    try:
        while True:
            first = sys.stdin.buffer.read(1)
            if not first:
                break
            rhdr = first + read_exact(sys.stdin.buffer, 3)
            rn = struct.unpack("<I", rhdr)[0]
            if not 1 <= rn <= MAX_REQ:
                raise ValueError("invalid request length")
            req = read_exact(sys.stdin.buffer, rn)
            cin.write(rhdr + req)
            cin.flush()
            hdr = read_exact(cout, 4)
            n = struct.unpack("<I", hdr)[0]
            if not 1 <= n <= MAX_REP:
                raise ValueError("invalid reply length")
            reply = read_exact(cout, n)
            if matches_request(root, req):
                log_trace(trace, {"event": "matched"})
                if not injected:
                    injected = True
                    receipt.write_bytes(reply)
                    log_trace(trace, {"event": "injected", "mode": mode})
                    if mode == "eof":
                        break
                    reply = b"\x00"
            sys.stdout.buffer.write(struct.pack("<I", len(reply)) + reply)
            sys.stdout.buffer.flush()
    finally:
        if not cin.closed:
            try:
                cin.close()
            except OSError:
                pass
        code = child.wait()
        if cout and not cout.closed:
            cout.close()
        log_trace(trace, {"event": "child_exit", "code": code, "pid": child.pid})
        if code != 0:
            sys.exit(code)

if __name__ == "__main__":
    main()
