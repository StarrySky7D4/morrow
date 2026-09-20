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

def main():
    root = Path(__file__).resolve().parent
    cfg = json.loads((root / "proxy.json").read_text(encoding="utf-8"))
    host, store, package = Path(cfg["host"]), Path(cfg["store"]), Path(cfg["package"])
    mode = cfg["mode"]
    if mode not in ("malformed", "eof"):
        die("invalid mode")
    armed_f = root / "armed.sha256"
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
            armed = armed_f.read_text(encoding="utf-8").strip() if armed_f.exists() else None
            if armed is not None and hashlib.sha256(req).hexdigest() == armed:
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
