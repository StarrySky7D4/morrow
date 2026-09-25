// Trusted local UI transport. Private identity and business frames stay in this
// device Worker; the only fetches load the application's static code and bundle.
import init, {install_workbench_opfs, BrowserWorkbench} from './core/morrow_web_core.js';
import {createDeviceIdentity, inspectDeviceIdentity, inspectDeviceLibrary, withDeviceIdentity, markDeviceIdentityReady} from './device-identity.mjs';
let host, opening = false, closed = false;
function exit(code, message) {
  closed = true;
  try { host?.free(); }
  catch (error) { code = 1; message = `${message} Host disposal failed: ${String(error)}`.trim(); }
  host = undefined;
  // Disposal failure must not strand the queue or leave the UI waiting for an
  // exit forever. Closing the Worker releases its remaining browser resources.
  try { self.postMessage({kind: 'exit', code, message}); }
  finally { self.close(); }
}
// Serialize startup as well as requests: close/error cannot race an asynchronous
// key read and then leave a newly opened storage owner behind.
let queue = Promise.resolve();
self.onmessage = ({data}) => { queue = queue.then(() => handle(data)); };
async function handle(data) {
  if (closed) return;
  try {
    if (data?.kind === 'open') {
      if (host || opening) throw Error('Host already selected');
      opening = true;
      if (typeof data.name !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(data.name) || typeof data.create !== 'boolean') {
        throw Error('Invalid local library selection');
      }
      // Opening a missing/damaged identity cannot become an implicit create.
      if (!data.create) await inspectDeviceIdentity(data.name);
      if (data.create && await inspectDeviceLibrary(data.name) !== 'empty') {
        throw Error('Local workspace already exists or requires recovery');
      }
      await init();
      await install_workbench_opfs();
      const response = await fetch(new URL('./workbench.morrowplugin', import.meta.url));
      if (!response.ok) throw Error('Workbench package unavailable');
      const archive = new Uint8Array(await response.arrayBuffer());
      if (data.create) await createDeviceIdentity(data.name);
      const identity = await withDeviceIdentity(data.name, ({name, logId, publicKey, seed, phase}) => {
        host = new BrowserWorkbench(name, data.create, logId, publicKey, seed, archive);
        return {name, logId, publicKey, phase};
      });
      host.checkpoint();
      host.integrity_check();
      // An interrupted prepared identity is retained. A later explicit open
      // can finish admission only if its original content store really exists.
      await markDeviceIdentityReady(identity);
      opening = false;
      self.postMessage({kind: 'ready'});
    } else if (data?.kind === 'request') {
      if (!host || opening || !(data.frame instanceof Uint8Array) || !data.frame.length || data.frame.length > 128 * 1024) {
        throw Error('Invalid host request');
      }
      let reply;
      try { reply = host.request(data.frame); }
      finally { data.frame.fill(0); }
      if (!reply.length || reply.length > 256 * 1024) throw Error('Invalid host response');
      self.postMessage({kind: 'reply', frame: reply}, [reply.buffer]);
    } else if (data?.kind === 'file-import' || data?.kind === 'file-export') {
      if (!host || opening) throw Error('Host is not ready for device files');
      // Expected file/permission failures reject this request, keeping the
      // original owner available for reconciliation and other local work.
      try {
        if (typeof data.card !== 'string') throw Error('Missing attachment owner');
        if (data.kind === 'file-import') {
          if (!(data.blob instanceof Blob) || typeof data.name !== 'string' || typeof data.mediaKind !== 'string') throw Error('Invalid selected file');
          const result = host.import_device_file(data.card, data.name, data.mediaKind, data.blob);
          try { self.postMessage({kind:'file-reply', assetId:result.id, size:result.size}); }
          finally { result.free(); }
        } else {
          if (typeof data.assetId !== 'string') throw Error('Missing attachment ID');
          const result = host.export_device_file(data.card, data.assetId);
          try { self.postMessage({kind:'file-reply', blob:result.blob, size:result.size, digest:result.digest}); }
          finally { result.free(); }
        }
      } catch (error) { self.postMessage({kind:'file-error', message:String(error)}); }
    } else if (data?.kind === 'close') {
      if (!host || opening) throw Error('Host is not ready to close');
      // Failure is reported as nonzero; pending durable audit events remain in
      // OPFS for the selected identity's normal recovery on its next open.
      host.close();
      exit(0, '');
    } else { throw Error('Unknown host message'); }
  } catch (error) { exit(1, String(error)); }
  finally { if (data?.frame instanceof Uint8Array) data.frame.fill(0); }
}
