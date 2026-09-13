"""Qualify the actual Windows native API node executable; local synthetic inputs only."""
from pathlib import Path
import hashlib
import http.client
import http.server
import os
import secrets
import subprocess
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "build/network-node/release/morrow-api-node.exe"


def main():
    if os.name != "nt":
        raise RuntimeError("This executable qualification is Windows-only")
    if not EXE.is_file():
        raise RuntimeError("Build network_node release with plugin-adapter first")
    evidence = ROOT / "build" / ("network-node-cli-" + uuid.uuid4().hex)
    evidence.mkdir()
    results = []

    def qualify(label, arguments, expected):
        token = secrets.token_urlsafe(32)
        token_file = evidence / (label + ".token")
        token_file.write_text(token, encoding="ascii")
        stdout = evidence / (label + ".stdout.log")
        stderr = evidence / (label + ".stderr.log")
        arguments = [str(a).replace("TOKEN_FILE", str(token_file)) for a in arguments]
        with stdout.open("wb") as out, stderr.open("wb") as err:
            process = subprocess.Popen([str(EXE), *arguments], cwd=ROOT, stdout=out, stderr=err,
                                       creationflags=subprocess.CREATE_NO_WINDOW)
            try:
                until = time.monotonic() + 10
                address = None
                while time.monotonic() < until:
                    if process.poll() is not None:
                        raise AssertionError(label + " exited before listening")
                    text = stdout.read_text(encoding="utf-8")
                    prefix = "Morrow API node listening on http://"
                    if prefix in text:
                        address = text.split(prefix, 1)[1].split("/", 1)[0]
                        break
                    time.sleep(0.02)
                assert address is not None, "listener timeout"
                host, port = address.rsplit(":", 1)
                for authorized in (False, True):
                    conn = http.client.HTTPConnection(host, int(port), timeout=5)
                    try:
                        headers = {"Content-Type": "application/octet-stream"}
                        if authorized:
                            headers["Authorization"] = "Bearer " + token
                        conn.request("POST", "/v1/invoke", body=bytes([0, 65, 255, 90]), headers=headers)
                        response = conn.getresponse()
                        body = response.read()
                        assert response.status == (200 if authorized else 401), label
                        if authorized:
                            assert body == expected, label
                    finally:
                        conn.close()
                assert token not in stdout.read_text(encoding="utf-8")
                assert token not in stderr.read_text(encoding="utf-8")
                results.append(label + ": PASS actual executable; unauthenticated 401; exact binary output")
            finally:
                # Stop only this helper PID. Graceful shutdown is separately tested at library level.
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=5)
                token_file.unlink()

    for language in ("c", "cpp", "rust"):
        package = ROOT / "sdk/compat/guest-v1-rc1" / (language + "-transform.mplugin")
        qualify(language, ["plugin", package, evidence / (language + "-data"),
                           "bytes.reverse", "TOKEN_FILE", "127.0.0.1:0"], bytes([90, 255, 65, 0]))

    class Upstream(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            assert self.headers.get("Authorization") is None, "node token leaked to upstream"
            body = self.rfile.read(int(self.headers["Content-Length"]))
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Content-Type", "application/octet-stream")
            self.end_headers()
            self.wfile.write(body)
        def log_message(self, *args):
            pass

    upstream = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    thread = threading.Thread(target=upstream.serve_forever, daemon=True)
    thread.start()
    try:
        qualify("relay", ["relay", "http://127.0.0.1:" + str(upstream.server_port) + "/external-api",
                          "TOKEN_FILE", "127.0.0.1:0", "--allow-local-upstream"], bytes([0, 65, 255, 90]))
    finally:
        upstream.shutdown()
        upstream.server_close()
        thread.join(timeout=5)
        assert not thread.is_alive()
    digest = hashlib.sha256(EXE.read_bytes()).hexdigest()
    report = "# Windows API node executable qualification\n\n" + "\n".join(results)
    report += "\n\nExecutable SHA-256: `" + digest + "`\n"
    report += "\nUses local synthetic APIs only. Process termination is cleanup, not graceful-stop proof.\n"
    (evidence / "result.md").write_text(report, encoding="utf-8")
    print(report)
    print(evidence)

if __name__ == "__main__":
    main()
