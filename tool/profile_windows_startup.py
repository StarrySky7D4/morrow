"""Measure owned Release startup processes against read-only fixture copies."""
import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--exe', type=Path, required=True)
parser.add_argument('--fixture', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runs', type=int, default=3)
parser.add_argument('--managed', action='store_true',
                    help='Register each isolated copy and measure the normal active-library route')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
user32 = ctypes.windll.user32
callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.EnumWindows.argtypes = [callback, wintypes.LPARAM]

def visible(pid):
    found = []
    @callback
    def visit(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True
    user32.EnumWindows(visit, 0)
    return bool(found)

results = []
for index in range(args.runs):
    target = args.output.resolve() / str(index)
    target.mkdir()
    with sqlite3.connect((args.fixture.resolve() / 'workbench.db').as_uri() + '?mode=ro', uri=True) as source:
        with sqlite3.connect(target / 'workbench.db') as destination:
            source.backup(destination)
    shutil.copy2(args.fixture / 'workbench.db.audit-key', target / 'workbench.db.audit-key')
    shutil.copytree(args.fixture / 'plugin-manager', target / 'plugin-manager')
    if args.managed:
        # Use the host's canonical registry encoder. Never copy an active
        # selection that could route the probe back into the original library.
        with (target / 'register.log').open('w') as log:
            subprocess.run([
                str(args.exe.resolve().parent / 'morrow-workbench-host.exe'),
                '--activate-library', str(target), str(target),
            ], check=True, timeout=60, creationflags=subprocess.CREATE_NO_WINDOW,
               stdout=log, stderr=log)
    report = target / 'timings.json'
    started = time.perf_counter()
    shown = None
    with (target / 'process.log').open('w') as log:
        proc = subprocess.Popen([
            str(args.exe.resolve()), f'--data-directory={target}', f'--startup-check={report}',
            *(['--managed-library'] if args.managed else []),
        ], creationflags=subprocess.CREATE_NO_WINDOW, stdout=log, stderr=log)
        while proc.poll() is None and time.perf_counter() - started < 45:
            if shown is None and visible(proc.pid):
                shown = (time.perf_counter() - started) * 1000
            time.sleep(.01)
        if proc.poll() is None:
            proc.kill()  # Only the process created by this probe.
            proc.wait()
            raise TimeoutError('Owned startup probe timed out')
    data = json.loads(report.read_text(encoding='utf-8'))
    data.update(first_visible_ms=shown, process_total_ms=(time.perf_counter()-started)*1000, exit_code=proc.returncode)
    results.append(data)
    print(json.dumps(data), flush=True)
    if proc.returncode != 0:
        raise RuntimeError('Startup qualification failed; inspect the isolated report')
(args.output / 'summary.json').write_text(json.dumps(results, indent=2), encoding='utf-8')
