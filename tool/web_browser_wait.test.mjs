import test from 'node:test';
import assert from 'node:assert/strict';
import {browserWaitPolicy, waitForBrowser} from './web_browser_wait.mjs';

test('wait budget follows the same explicit CPU rate and is capped', () => {
  assert.deepEqual(browserWaitPolicy([]), {cpuRate:1, baselineMs:45000, timeoutMs:45000});
  assert.deepEqual(browserWaitPolicy(['--slow-ui','4']), {cpuRate:4, baselineMs:45000, timeoutMs:180000});
  assert.equal(browserWaitPolicy(['--slow-ui']).cpuRate, 4);
  assert.equal(browserWaitPolicy(['--slow-ui','2']).timeoutMs, 90000);
  assert.equal(browserWaitPolicy(['--site','https://example.test/','--slow-ui','4']).timeoutMs, 180000);
  for (const invalid of ['0','5','nope','Infinity']) assert.throws(() => browserWaitPolicy(['--slow-ui',invalid]));
});

test('slow progress is reported at the original budget and still must pass', async () => {
  let elapsed = 0;
  const events = [];
  await waitForBrowser(async () => elapsed >= 70, 'music', {cpuRate:4, baselineMs:45, timeoutMs:180}, {
    now:() => elapsed, pause:async () => {elapsed += 10;}, observe:async event => events.push(event),
  });
  assert.deepEqual(events.map(({status}) => status), ['baseline-exceeded','passed']);
  assert.equal(events[0].elapsedMs, 50);
  assert.equal(events[1].elapsedMs, 70);
});

test('the scaled deadline is fixed rather than renewed by observations', async () => {
  let elapsed = 0;
  const events = [];
  await assert.rejects(waitForBrowser(async () => false, 'music', {cpuRate:4, baselineMs:45, timeoutMs:180}, {
    now:() => elapsed, pause:async () => {elapsed += 10;}, observe:async event => {events.push(event); elapsed += 5;},
  }), /Timed out: music/);
  assert.deepEqual(events.map(({status}) => status), ['baseline-exceeded','failed']);
  assert.equal(events[1].elapsedMs, 185);
});
