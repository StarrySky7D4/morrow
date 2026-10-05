import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, readFile, rm, mkdir} from 'node:fs/promises';
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


test('unavailable initial checkpoint cannot skip the operation or replace its error', async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'morrow-timing-missing-'));
  try {
    const warnings = [];
    const timed = qualificationTimer(path.join(dir, 'missing', 'timings.json'), () => {}, line => warnings.push(line));
    assert.equal(await timed('success without diagnostics', async () => 42), 42);
    const originalError = new Error('browser assertion');
    await assert.rejects(timed('failure without diagnostics', async () => {throw originalError;}), error => error === originalError);
    assert.equal(warnings.length, 4);
  } finally {
    await rm(dir, {recursive: true, force: true});
  }
});

test('failed final checkpoint and throwing logger preserve the browser outcome', async () => {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'morrow-timing-final-'));
  try {
    const file = path.join(dir, 'timings.json');
    const warnings = [];
    const timed = qualificationTimer(file, () => {throw new Error('logging unavailable');}, line => warnings.push(line));
    const originalError = new Error('original browser failure');
    await assert.rejects(timed('browser failure', async () => {
      await mkdir(`${file}.tmp`);
      throw originalError;
    }), error => error === originalError);
    assert.equal(warnings.length, 3);
    assert.equal(await timed('browser success', async () => 42), 42);
  } finally {
    await rm(dir, {recursive: true, force: true});
  }
});
