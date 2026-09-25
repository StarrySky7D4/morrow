// Qualification fixture only. The fixed key is NEVER a product identity.
import init, {install_opfs, BrowserWorkbench} from './main/morrow_web_core.js';
const restore = new URL(self.location.href).searchParams.has('restore');
let host;
let result;
let stage = 'initialize';
try {
  await init();
  try { await install_opfs(); }
  catch (error) {
    if (restore) { self.postMessage(`RETRY: ${error}`); self.close(); }
    throw error;
  }
  const read = async name => {
    const response = await fetch(name);
    if (!response.ok) throw Error(`fixture fetch: ${name}`);
    return new Uint8Array(await response.arrayBuffer());
  };
  const [archive, publicKey] = await Promise.all([read('./workbench.morrowplugin'), read('./fixture-public.bin')]);
  stage = 'open';
  const open = (create, byte = 42) => {
    const seed = new Uint8Array(32).fill(byte);
    try { return new BrowserWorkbench('protocol-fixture', create, 'browser-protocol-fixture', publicKey, seed, archive); }
    finally { seed.fill(0); }
  };
  if (restore) {
    let rejected = false;
    try { const unexpected = open(false, 43); unexpected.free(); }
    catch { rejected = true; }
    if (!rejected) throw Error('wrong key accepted');
  }
  host = open(!restore);
  stage = 'duplicate owner';
  let rejected = false;
  try { const unexpected = open(false); unexpected.free(); }
  catch { rejected = true; }
  if (!rejected) throw Error('duplicate database owner accepted');
  stage = 'protocol qualification';
  host.qualify(restore);
  stage = 'flush';
  host.finish();
  stage = 'integrity';
  host.integrity_check();
  stage = 'close';
  host.close();
  let closedRejected = false;
  try { host.integrity_check(); } catch { closedRejected = true; }
  if (!closedRejected) throw Error('closed owner accepted');
  result = restore
    ? 'PASS: separate Worker restored device-local card, long draft, locale and approval; wrong key rejected; audit integrity checked'
    : 'PASS: shared host binary protocol create/edit/stale conflict, segmented Unicode draft, locale, approval, audited OPFS and exclusive ownership';
} catch (error) { result = `FAIL: ${stage}: ${error?.stack ?? String(error)}`; }
finally { host?.free(); }
self.postMessage(result);
self.close();
