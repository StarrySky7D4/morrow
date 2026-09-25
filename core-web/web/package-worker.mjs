// Browser parity qualification only; not an application entry point.
import init, { install_opfs, BrowserStore, BrowserTransformPackage, BrowserPluginRegistry } from './main/morrow_web_core.js';
import { qualifyDependencies } from './package-dependencies.mjs';
const bytes = async name => {
  const response = await fetch(`./vectors/${name}`);
  if (!response.ok) throw Error(`Missing fixture ${name}`);
  return new Uint8Array(await response.arrayBuffer());
};
const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
function rejects(callback, fragment) {
  try { callback(); } catch (error) {
    if (!String(error).includes(fragment)) throw error;
    return;
  }
  throw Error(`Expected rejection: ${fragment}`);
}
let store, prepared, registry, result, openingPool = true;
const restoring = new URL(import.meta.url).searchParams.has('restore');
try {
  await init();
  await install_opfs();
  openingPool = false;
  store = new BrowserStore('package-parity', !restoring, 1024);
  if (!restoring) store.create_local('seed-protected', 'protected', 'unchanged');
  const archive = await bytes('package.bin');
  const digest = await bytes('digest.bin');
  const wrong = digest.slice(); wrong[0] ^= 1;
  rejects(() => new BrowserTransformPackage(archive, wrong, 20000000, 16 * 1024 * 1024), 'digest');
  prepared = new BrowserTransformPackage(archive, digest, 20000000, 16 * 1024 * 1024);
  if (!equal(prepared.digest(), digest)) throw Error('Package identity changed');
  const cases = await (await fetch('./vectors/cases.json')).json();
  for (const name of cases) {
    const actual = prepared.transform(store, await bytes(`${name}.request.bin`));
    if (!equal(actual, await bytes(`${name}.expected.bin`))) throw Error(`Native/Web result mismatch: ${name}`);
  }
  const unauthorized = await bytes('content-command.bin');
  rejects(() => prepared.transform(store, unauthorized), 'refuses content commands');
  const request = await bytes('create.request.bin');
  const tiny = new BrowserTransformPackage(archive, digest, 1, 16 * 1024 * 1024);
  try { rejects(() => tiny.transform(store, request), 'Package execution'); }
  finally { tiny.free(); }
  // A failed guest must not poison the OPFS owner or leak per-call connections.
  for (let i = 0; i < 130; i++) prepared.transform(store, request);
  if (store.card_title('protected') !== 'unchanged' || store.card_revision('protected') !== 1n) {
    throw Error('Pure transform modified protected content');
  }
  store.check();
  registry = new BrowserPluginRegistry('parity', !restoring);
  rejects(() => new BrowserPluginRegistry('parity', false), 'StorageBusy');
  const check = async name => {
    if (!equal(registry.snapshot(), await bytes(`${name}.registry.bin`))) throw Error(`Native/Web registry mismatch: ${name}`);
    const revision = BigInt(await (await fetch(`./vectors/${name}.revision.txt`)).text());
    if (registry.revision() !== revision) throw Error(`Registry revision mismatch: ${name}`);
  };
  const actualId = await (await fetch('./vectors/actual-id.txt')).text();
  const run = () => registry.transform(store, actualId, request);
  if (restoring) {
    await check('actual-disabled');
    rejects(run, 'plugin disabled');
    registry.set_enabled(actualId, digest, true, registry.revision());
    if (!equal(run(), await bytes('create.expected.bin'))) throw Error('Reopened package execution mismatch');
    registry.set_enabled(actualId, digest, false, registry.revision());
  } else {
    if (registry.revision() !== 0n || registry.snapshot().length !== 0) throw Error('Fresh registry is not empty');
    const first = registry.install(await bytes('first.package.bin'));
    const second = registry.install(await bytes('second.package.bin'));
    registry.install(archive);
    if (registry.revision() !== 0n) throw Error('Install implicitly selected package');
    const id = 'org.example.registry';
    registry.select(first, 0n); await check('selected');
    rejects(() => registry.approve(id, first, new Int32Array([4]), registry.revision()), 'approval exceeds declaration');
    registry.approve(id, first, new Int32Array([1, 2]), registry.revision()); await check('approved');
    registry.set_enabled(id, first, true, registry.revision()); await check('enabled');
    const stale = registry.revision();
    registry.select(second, stale); await check('upgraded');
    rejects(() => registry.set_enabled(id, first, true, registry.revision()), 'RevisionConflict');
    rejects(() => registry.set_enabled(id, second, true, stale), 'RevisionConflict');
    rejects(() => registry.set_enabled(id, second, true, 18446744073709551615n), 'RevisionConflict');
    await check('upgraded');
    registry.set_enabled(id, second, true, registry.revision()); await check('enabled-v2');
    registry.approve(id, second, new Int32Array(), registry.revision()); await check('revoked');
    registry.set_enabled(id, second, false, registry.revision()); await check('disabled');
    registry.remove(id, registry.revision()); await check('removed');
    registry.select(first, registry.revision()); await check('reselected');
    registry.select(digest, registry.revision()); await check('actual-selected');
    rejects(run, 'plugin disabled');
    registry.set_enabled(actualId, digest, true, registry.revision()); await check('actual-enabled');
    if (!equal(run(), await bytes('create.expected.bin'))) throw Error('Managed package execution mismatch');
    const session = registry.activate(store, actualId, registry.revision());
    if (!equal(registry.run_session(store, session, request), await bytes('create.expected.bin'))) throw Error('Live session result mismatch');
    registry.set_enabled(actualId, digest, false, registry.revision()); await check('actual-disabled');
    rejects(() => registry.run_session(store, session, request), 'Denied');
    registry.close_session(store, session); session.free();
    rejects(run, 'plugin disabled');
  }
  await qualifyDependencies(store, restoring, bytes, equal, rejects);
  if (store.card_title('protected') !== 'unchanged' || store.card_revision('protected') !== 1n) throw Error('Dependency execution changed protected content');
  result = `PASS: ${restoring ? 'Worker reopen, persisted approvals and execution revocation' : `${cases.length} actual native/Web guest vectors, execution bounds, 12 registry snapshots and original Rust/C/C++ dependency guests`}`;
} catch (error) {
  const retry = restoring && openingPool && /createSyncAccessHandle|NoModificationAllowedError/.test(String(error));
  result = `${retry ? 'RETRY' : 'FAIL'}: ${error?.stack ?? error}`;
} finally {
  try { if (registry && store) registry.close_all(store); }
  catch (error) { result = `FAIL: closing plugin sessions: ${error?.stack ?? error}`; }
  registry?.free();
  prepared?.free();
  store?.free();
}
postMessage(result);
close();
