// Trusted core worker. The bytes are Cap'n Proto, with no JSON business payload.
const ready = WebAssembly.instantiateStreaming(fetch('./morrow_core.wasm'), {}).then(value => value.instance.exports);
self.onmessage = async ({ data }) => {
  let core, input = 0, output = 0;
  try {
    if (!(data instanceof ArrayBuffer) || data.byteLength === 0 || data.byteLength > 65536) throw 2;
    core = await ready;
    input = core.morrow_buffer_new(data.byteLength);
    if (!input) throw 3;
    new Uint8Array(core.memory.buffer, core.morrow_buffer_ptr(input), data.byteLength).set(new Uint8Array(data));
    output = core.morrow_buffer_process(input);
    if (!output) throw core.morrow_buffer_status(input);
    const length = core.morrow_buffer_len(output);
    if (!length || length > 65536) throw 3;
    const bytes = new Uint8Array(core.memory.buffer, core.morrow_buffer_ptr(output), length).slice();
    self.postMessage(bytes.buffer, [bytes.buffer]);
  } catch (status) { self.postMessage(typeof status === 'number' ? status : 255); }
  finally {
    if (output) core.morrow_buffer_free(output);
    if (input) core.morrow_buffer_free(input);
  }
};
