import {writeFile, rename} from 'node:fs/promises';
import {performance} from 'node:perf_hooks';

// Checkpoint each operation so a CI step cancellation still leaves the last
// completed operation and the operation that was running. No UI/data payloads.
export function qualificationTimer(file, log = console.log) {
  const startedAt = new Date().toISOString();
  const started = performance.now();
  const operations = [];
  const checkpoint = async () => {
    await writeFile(`${file}.tmp`, JSON.stringify({startedAt, operations}, null, 2));
    await rename(`${file}.tmp`, file);
  };
  return async (label, action) => {
    const operation = {label, elapsedMs: Math.round(performance.now() - started), outcome: 'running'};
    operations.push(operation);
    await checkpoint();
    log(`Theme timing: start ${label} (+${operation.elapsedMs}ms)`);
    const before = performance.now();
    try {
      const result = await action();
      operation.outcome = 'passed';
      return result;
    } catch (error) {
      operation.outcome = 'failed';
      throw error;
    } finally {
      operation.durationMs = Math.round(performance.now() - before);
      await checkpoint();
      log(`Theme timing: ${operation.outcome} ${label} (${operation.durationMs}ms)`);
    }
  };
}
