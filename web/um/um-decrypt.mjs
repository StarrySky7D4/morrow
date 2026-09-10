/** WASM ABI v1 adapter. Audio/key operations never perform I/O. */
export const UmFormat = Object.freeze({
  auto: 0, ncm: 1, qmc: 2, kgm: 3, kwm: 4, tm: 5,
  xiami: 6, ximalaya: 7, raw: 8, qmcPayload: 9,
});
export const UmAudioFormat = Object.freeze({
  mp3: 1, flac: 2, ogg: 3, mp4: 4, wav: 5, wma: 6, dff: 7, aac: 8, ape: 9,
});
const messages = {
  1: 'Invalid argument or disposed decoder/library', 2: 'Truncated container',
  3: 'Invalid container header or footer', 4: 'Unsupported format',
  5: 'Unsupported encryption version or slot', 6: 'External key required',
  7: 'Invalid key, envelope or padding', 8: 'Decrypted audio header is not recognized',
  9: 'Chunk outside audio range', 10: 'Resource limit exceeded',
  255: 'Internal WASM error; create a new library instance',
};
export class UmDecryptError extends Error {
  constructor(code, options) {
    super(messages[code] ?? 'Unknown decryption error', options);
    this.name = 'UmDecryptError';
    this.code = code;
  }
}
function check(code) { if (code !== 0) throw new UmDecryptError(code); }
function bytes(value) {
  if (!(value instanceof Uint8Array)) throw new UmDecryptError(1);
  return value;
}
function integer(value, min, max, code = 1) {
  if (!Number.isSafeInteger(value) || value < min || value > max) throw new UmDecryptError(code);
}
const MAX_BUFFER = 512 * 1024 * 1024;
const INTERNAL = Symbol('internal constructor');
const finalizer = typeof FinalizationRegistry === 'function'
  ? new FinalizationRegistry(({ runtime, handle }) => {
      try { runtime.freeHandle(handle); } catch { /* Explicit disposal remains preferred. */ }
    })
  : null;

/** A copy of key bytes. ekey expects base64 text, decoded expects audio-key bytes. */
export class UmKey {
  #kind;
  #data;
  constructor(token, kind, data) {
    if (token !== INTERNAL) throw new UmDecryptError(1);
    this.#kind = kind;
    this.#data = data;
  }
  static none() { return new UmKey(INTERNAL, 0, new Uint8Array()); }
  static ekey(text) {
    if (typeof text !== 'string') throw new UmDecryptError(1);
    if (text.length > 16384) throw new UmDecryptError(10);
    return new UmKey(INTERNAL, 1, new TextEncoder().encode(text));
  }
  static decoded(data) {
    bytes(data);
    if (data.length > 16384) throw new UmDecryptError(10);
    return new UmKey(INTERNAL, 2, Uint8Array.from(data));
  }
  static unpack(key) {
    if (!(key instanceof UmKey)) throw new UmDecryptError(1);
    return [key.#kind, key.#data];
  }
}

class Runtime {
  constructor(instance) {
    this.api = instance.exports;
    this.handles = new Set();
    this.closed = false;
    this.failed = false;
    for (const name of ['um_abi_version', 'um_buffer_alloc', 'um_buffer_free',
      'um_decoder_new', 'um_decoder_info', 'um_decoder_decrypt', 'um_decoder_free', 'um_key_hint']) {
      if (typeof this.api[name] !== 'function') throw new Error(`Missing WASM export: ${name}`);
    }
    if (!(this.api.memory instanceof WebAssembly.Memory) || this.api.um_abi_version() !== 1) {
      throw new Error('Incompatible um-decrypt WASM ABI (expected version 1)');
    }
  }
  ensureOpen() {
    if (this.failed) throw new UmDecryptError(255);
    if (this.closed) throw new UmDecryptError(1);
  }
  call(name, ...args) {
    this.ensureOpen();
    try { return this.api[name](...args); }
    catch (error) {
      // wasm32-unknown-unknown aborts on panic. Never reuse state after a trap.
      this.failed = true;
      throw new UmDecryptError(255, { cause: error });
    }
  }
  // Always create views from the CURRENT buffer. An allocation may grow memory
  // and detach every old Uint8Array/DataView into the previous buffer.
  view() { return new DataView(this.api.memory.buffer); }
  output(pointer, length) { return new Uint8Array(this.api.memory.buffer, pointer, length).slice(); }
  freeHandle(handle) {
    if (!this.handles.delete(handle)) return;
    if (!this.closed && !this.failed) check(this.call('um_decoder_free', handle));
  }
  dispose() {
    if (this.closed) return;
    try { for (const handle of this.handles) this.freeHandle(handle); }
    finally { this.handles.clear(); this.closed = true; }
  }
}

class Buffers {
  constructor(runtime) { this.runtime = runtime; this.allocations = []; }
  alloc(length) {
    integer(length, 0, MAX_BUFFER, 10);
    if (length === 0) return 0;
    const pointer = this.runtime.call('um_buffer_alloc', length) >>> 0;
    if (pointer === 0) throw new UmDecryptError(10);
    this.allocations.push([pointer, length]);
    return pointer;
  }
  copy(input) {
    bytes(input);
    const pointer = this.alloc(input.length);
    if (input.length) new Uint8Array(this.runtime.api.memory.buffer, pointer, input.length).set(input);
    return pointer;
  }
  dispose() {
    if (!this.runtime.failed) {
      for (const [pointer, length] of this.allocations.reverse()) {
        this.runtime.call('um_buffer_free', pointer, length);
      }
    }
    this.allocations.length = 0;
  }
}

export class UmLibrary {
  #runtime;
  constructor(token, runtime) {
    if (token !== INTERNAL) throw new UmDecryptError(1);
    this.#runtime = runtime;
  }
  /** Instantiate an already compiled module. Each library has independent memory. */
  static fromModule(module) {
    if (!(module instanceof WebAssembly.Module)) throw new UmDecryptError(1);
    if (WebAssembly.Module.imports(module).length !== 0) {
      throw new Error('um-decrypt expects a standalone module with no host imports');
    }
    return new UmLibrary(INTERNAL, new Runtime(new WebAssembly.Instance(module, {})));
  }
  /** Compile caller-supplied WASM bytes; no network or filesystem access. */
  static async fromBytes(wasm) {
    if (!(wasm instanceof Uint8Array) && !(wasm instanceof ArrayBuffer)) throw new UmDecryptError(1);
    return UmLibrary.fromModule(await WebAssembly.compile(wasm));
  }
  /** Load only the WASM program asset. Audio/key data is never fetched or sent. */
  static async load(url = new URL('./um_decrypt.wasm', import.meta.url)) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`WASM asset load failed: HTTP ${response.status}`);
    return UmLibrary.fromBytes(await response.arrayBuffer());
  }
  prepare(container, { format = UmFormat.auto, key = UmKey.none() } = {}) {
    this.#runtime.ensureOpen();
    bytes(container);
    integer(format, 0, 9);
    const [kind, material] = UmKey.unpack(key);
    const buffers = new Buffers(this.#runtime);
    let handle = 0n;
    try {
      const source = buffers.copy(container);
      const keyPointer = buffers.copy(material);
      const out = buffers.alloc(8);
      check(this.#runtime.call('um_decoder_new', source, container.length, format,
        keyPointer, material.length, kind, out));
      handle = this.#runtime.view().getBigUint64(out, true);
      this.#runtime.handles.add(handle);
      const infoPointer = buffers.alloc(24);
      check(this.#runtime.call('um_decoder_info', handle, infoPointer));
      const view = this.#runtime.view();
      const info = Object.freeze({
        audioOffset: Number(view.getBigUint64(infoPointer, true)),
        audioLength: Number(view.getBigUint64(infoPointer + 8, true)),
        format: view.getUint32(infoPointer + 16, true),
        audioFormat: view.getUint32(infoPointer + 20, true),
      });
      return new UmDecoder(INTERNAL, this.#runtime, handle, container.length, info);
    } catch (error) {
      if (handle !== 0n) this.#runtime.freeHandle(handle);
      throw error;
    } finally { buffers.dispose(); }
  }
  decrypt(container, options) {
    const decoder = this.prepare(container, options);
    try { return Object.freeze({ info: decoder.info, bytes: decoder.decrypt(container) }); }
    finally { decoder.dispose(); }
  }
  keyHint(container, { format = UmFormat.auto } = {}) {
    this.#runtime.ensureOpen();
    bytes(container);
    integer(format, 0, 9);
    const buffers = new Buffers(this.#runtime);
    try {
      const data = buffers.copy(container);
      // size_t is 32-bit in wasm32; handle/offset fields remain 64-bit.
      const countPointer = buffers.alloc(4);
      check(this.#runtime.call('um_key_hint', data, container.length, format, 0, 0, countPointer));
      const length = this.#runtime.view().getUint32(countPointer, true);
      if (length === 0) return null;
      const out = buffers.alloc(length);
      check(this.#runtime.call('um_key_hint', data, container.length, format, out, length, countPointer));
      return new TextDecoder('utf-8', { fatal: true }).decode(this.#runtime.output(out, length));
    } finally { buffers.dispose(); }
  }
  /** Invalidates this library and all its prepared decoders. */
  dispose() { this.#runtime.dispose(); }
}

export class UmDecoder {
  #runtime;
  #handle;
  #sourceLength;
  #info;
  #disposed = false;
  constructor(token, runtime, handle, length, info) {
    if (token !== INTERNAL) throw new UmDecryptError(1);
    this.#runtime = runtime;
    this.#handle = handle;
    this.#sourceLength = length;
    this.#info = info;
    finalizer?.register(this, { runtime, handle }, this);
  }
  get info() { return this.#info; }
  #ensureOpen() {
    if (this.#disposed) throw new UmDecryptError(1);
    this.#runtime.ensureOpen();
  }
  decryptChunk(encrypted, { offset = 0 } = {}) {
    this.#ensureOpen();
    bytes(encrypted);
    integer(offset, 0, this.#info.audioLength, 9);
    if (encrypted.length > this.#info.audioLength - offset) throw new UmDecryptError(9);
    const buffers = new Buffers(this.#runtime);
    try {
      const pointer = buffers.copy(encrypted);
      check(this.#runtime.call('um_decoder_decrypt', this.#handle, BigInt(offset), pointer, encrypted.length));
      return this.#runtime.output(pointer, encrypted.length);
    } finally { buffers.dispose(); }
  }
  decryptChunkInPlace(encrypted, options) { encrypted.set(this.decryptChunk(encrypted, options)); }
  decrypt(container) {
    this.#ensureOpen();
    bytes(container);
    if (container.length !== this.#sourceLength) throw new UmDecryptError(1);
    return this.decryptChunk(container.subarray(this.#info.audioOffset,
      this.#info.audioOffset + this.#info.audioLength));
  }
  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    finalizer?.unregister(this);
    this.#runtime.freeHandle(this.#handle);
  }
}

/** Optional Dart JS-interop bridge. Call once before starting the Dart app. */
export function installDartBridge() {
  const safe = (fn) => {
    try { return { code: 0, value: fn() }; }
    catch (error) { return { code: error instanceof UmDecryptError ? error.code : 255, value: null }; }
  };
  const safeAsync = async (fn) => {
    try { return { code: 0, value: await fn() }; }
    catch (error) { return { code: error instanceof UmDecryptError ? error.code : 255, value: null }; }
  };
  const key = (kind, data) => {
    if (kind === 0) return UmKey.none();
    if (kind === 1) return UmKey.ekey(new TextDecoder().decode(data));
    if (kind === 2) return UmKey.decoded(data);
    throw new UmDecryptError(1);
  };
  const decoderAdapter = (decoder) => ({
    info: decoder.info,
    decrypt: (data) => safe(() => decoder.decrypt(data)),
    decryptChunk: (data, offset) => safe(() => decoder.decryptChunk(data, { offset })),
    dispose: () => safe(() => decoder.dispose()),
  });
  const libraryAdapter = (library) => ({
    prepare: (data, format, kind, material) => safe(() => decoderAdapter(library.prepare(data, { format, key: key(kind, material) }))),
    keyHint: (data, format) => safe(() => library.keyHint(data, { format })),
    dispose: () => safe(() => library.dispose()),
  });
  globalThis.umDecryptWasm = Object.freeze({
    abiVersion: 1,
    load: (url) => safeAsync(async () => libraryAdapter(await UmLibrary.load(url))),
    fromBytes: (data) => safeAsync(async () => libraryAdapter(await UmLibrary.fromBytes(data))),
  });
}
