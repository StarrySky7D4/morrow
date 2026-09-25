// Qualification uses checked-in original SDK guest binaries; never recompiles them.
import { BrowserPluginRegistry } from './main/morrow_web_core.js';
export async function qualifyDependencies(store, restoring, bytes, equal, rejects) {
  const registry = new BrowserPluginRegistry('dependency-parity', !restoring);
  const provider = 'org.morrow.compat.rust.provider';
  try {
    const providerDigest = registry.install(await bytes('dependency-provider.package.bin'));
    if (!restoring) {
      registry.select(providerDigest, registry.revision());
      registry.set_enabled(provider, providerDigest, true, registry.revision());
    } else {
      rejects(() => registry.activate(store, 'org.morrow.compat.rust.dependency', registry.revision()), 'plugin disabled');
      registry.set_enabled(provider, providerDigest, true, registry.revision());
    }
    for (const language of ['rust', 'c', 'cpp']) {
      const id = `org.morrow.compat.${language}.dependency`;
      const digest = registry.install(await bytes(`dependency-${language}.package.bin`));
      if (!restoring) {
        registry.select(digest, registry.revision());
        registry.set_enabled(id, digest, true, registry.revision());
        rejects(() => registry.activate(store, id, registry.revision()), 'NotFound');
        registry.approve_dependency(id, digest, 'reverse', provider, providerDigest, registry.revision());
      }
      const session = registry.activate(store, id, registry.revision());
      try {
        const input = await bytes(`dependency-${language}.request.bin`);
        const actual = registry.run_dependency_session(store, session, input, 1);
        if (!equal(actual, await bytes(`dependency-${language}.expected.bin`))) throw Error(`Original ${language} dependency result mismatch`);
        rejects(() => registry.run_dependency_session(store, session, input, 0), 'Invalid dependency budget');
        // Revoking the provider stops its live dependent immediately.
        registry.set_enabled(provider, providerDigest, false, registry.revision());
        rejects(() => registry.run_dependency_session(store, session, input, 1), 'Denied');
        registry.set_enabled(provider, providerDigest, true, registry.revision());
        // Enabling again must not resurrect the old dependent session.
        rejects(() => registry.run_dependency_session(store, session, input, 1), 'Denied');
      } finally { registry.close_session(store, session); session.free(); }
    }
    registry.set_enabled(provider, providerDigest, false, registry.revision());
  } finally { registry.close_all(store); registry.free(); }
}
