import {setTimeout as delay} from 'node:timers/promises';

export function browserWaitPolicy(argv = process.argv) {
  const index = argv.indexOf('--slow-ui');
  const argument = index < 0 ? null : argv[index + 1];
  const cpuRate = index < 0 ? 1 : argument == null || argument.startsWith('--') ? 4 : Number(argument);
  if (!Number.isFinite(cpuRate) || cpuRate < 1 || cpuRate > 4) {
    throw Error('--slow-ui requires a CPU rate between 1 and 4');
  }
  const baselineMs = argv.includes('--site') ? 90000 : 45000;
  return {cpuRate, baselineMs, timeoutMs: Math.min(baselineMs * cpuRate, 180000)};
}

export async function waitForBrowser(check, label, policy, {
  now = Date.now,
  pause = () => delay(150),
  observe = async () => {},
} = {}) {
  const started = now();
  const deadline = started + policy.timeoutMs;
  let slow = false;
  while (now() < deadline) {
    const passed = await check();
    const elapsedMs = now() - started;
    if (!slow && elapsedMs >= policy.baselineMs) {
      slow = true;
      await observe({label, status:'baseline-exceeded', elapsedMs, ...policy});
    }
    if (passed) {
      if (slow) await observe({label, status:'passed', elapsedMs:now() - started, ...policy});
      return;
    }
    await pause();
  }
  await observe({label, status:'failed', elapsedMs:now() - started, ...policy});
  throw Error('Timed out: ' + label);
}
