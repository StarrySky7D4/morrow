import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, readFile, rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {qualificationTimer} from './web_qualification_timing.mjs';

test('operation timing checkpoints running, passed and failed without changing results', async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'morrow-timing-'));
  try {
    const file = path.join(dir, 'timings.json');
    const logs = [];
    const timed = qualificationTimer(file, line => logs.push(line));
    const inspect = async () => JSON.parse(await readFile(file, 'utf8'));
    assert.equal(await timed('success', async () => {
      assert.equal((await inspect()).operations[0].outcome, 'running');
      return 42;
    }), 42);
    const originalError = new Error('original failure');
    await assert.rejects(timed('failure', async () => {throw originalError;}), error => error === originalError);
    const result = await inspect();
    assert.equal(typeof result.startedAt, 'string');
    assert.deepEqual(result.operations.map(({label, outcome}) => ({label, outcome})), [
      {label:'success', outcome:'passed'}, {label:'failure', outcome:'failed'},
    ]);
    assert.ok(result.operations.every(entry => entry.durationMs >= 0 && entry.elapsedMs >= 0));
    assert.equal(logs.length, 4);
  } finally {
    await rm(dir, {recursive: true, force: true});
  }
});
