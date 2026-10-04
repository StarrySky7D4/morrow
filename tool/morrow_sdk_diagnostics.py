"""Standalone diagnostics for an explicitly trusted local host; no guest run.

The adjacent, original sdk_profiles.py owns protocol validation and process bounds.
This entry point does not find, download or build a host or grant any authority.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys


def load_profiles():
    # An explicit adjacent source load works under Python -I, which omits the
    # script directory from sys.path. Never fall back to a repository or PYTHONPATH.
    sys.dont_write_bytecode = True  # Keep the immutable bundle clean even without -B.
    path = Path(__file__).resolve().with_name('sdk_profiles.py')
    spec = importlib.util.spec_from_file_location('_morrow_adjacent_sdk_profiles', path)
    if spec is None or spec.loader is None:
        raise ImportError('adjacent sdk_profiles.py is unavailable')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    profiles = commands.add_parser('profiles', help='read compiled profiles; no authority')
    preflight = commands.add_parser('preflight', help='static package preparation; no guest execution')
    preflight.add_argument('package', type=Path)
    for command in (profiles, preflight):
        command.add_argument('--host', required=True, type=Path,
                             help='explicit path to a trusted local host executable')
    args = parser.parse_args(argv)  # Ordinary argparse usage errors retain exit 2.
    try:
        sdk = load_profiles()
        result = (sdk.query(args.host) if args.command == 'profiles'
                  else sdk.preflight(args.host, args.package))
        status = 2 if args.command == 'preflight' and result['preflight']['status'] == 'rejected' else 0
        rendered = json.dumps(result, sort_keys=True)
    except (OSError, ValueError, UnicodeError, ImportError, SyntaxError,
            RuntimeError, subprocess.SubprocessError) as error:
        print('ERROR:', error, file=sys.stderr)
        return 1
    print(rendered)  # Emit a receipt only after the original consumer validates it.
    return status


if __name__ == '__main__':
    raise SystemExit(main())
