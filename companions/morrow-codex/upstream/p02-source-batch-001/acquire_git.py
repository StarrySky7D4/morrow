import datetime
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
SOURCES = {'codex': ('https://github.com/openai/codex.git', '44fe510ce3ee61c8ef623adcbf89b901c73ddd61'), 'cc-switch': ('https://github.com/farion1231/cc-switch.git', '846de29c13ac4d65f164db8c15dd5fd58e29f972')}
name = sys.argv[1]
url, commit = SOURCES[name]
dest = BATCH / name
env = dict(os.environ)
env.update({'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_SYSTEM': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1', 'GIT_TERMINAL_PROMPT': '0', 'GIT_ASKPASS': '', 'SSH_ASKPASS': ''})
prefix = ['git', '-c', 'credential.helper=', '-c', 'core.askpass=', '-c', 'http.version=HTTP/1.1', '-c', 'http.sslBackend=openssl', '-c', 'http.lowSpeedLimit=1', '-c', 'http.lowSpeedTime=120', '-c', 'core.autocrlf=false', '-c', 'core.longpaths=true', '-c', 'core.symlinks=false']
steps = [['git', '-c', 'init.templateDir=', '-c', 'init.defaultBranch=source', 'init', str(dest)], prefix + ['-C', str(dest), 'fetch', '--no-tags', '--depth', '1', url, commit], prefix + ['-C', str(dest), 'checkout', '--detach', 'FETCH_HEAD']]
results = []
for index, command in enumerate(steps):
    stdout = RECEIPTS / f'{name}-git-{index}.stdout.txt'
    stderr = RECEIPTS / f'{name}-git-{index}.stderr.txt'
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    print(f'{name}: start step {index}', flush=True)
    with stdout.open('wb') as out, stderr.open('wb') as err:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=out, stderr=err)
    results.append({'command_argv': command, 'cwd': str(ROOT), 'started_at': started, 'ended_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'exit_code': result.returncode, 'stdout': str(stdout.relative_to(ROOT)), 'stderr': str(stderr.relative_to(ROOT))})
    (RECEIPTS / f'{name}-git-attempt.json').write_text(json.dumps({'schema_version': 1, 'batch': 'p02-source-batch-001', 'repository': url, 'commit': commit, 'isolated_git_config': True, 'steps': results}, indent=2) + '\n', encoding='utf-8')
    print(f'{name}: step {index} exit {result.returncode}', flush=True)
    if result.returncode:
        print(stderr.read_text(encoding='utf-8', errors='replace')[-4000:], flush=True)
        raise SystemExit(result.returncode)
print('fixed checkout completed', dest, flush=True)
