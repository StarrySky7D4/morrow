import 'dart:js_interop';
import 'dart:typed_data';
import 'types.dart';
export 'types.dart';

@JS('umDecryptWasm')
external _Bridge? get _installedBridge;

extension type _Bridge(JSObject _) implements JSObject {
  external int get abiVersion;
  external JSPromise<_Result> load(JSString url);
  external JSPromise<_Result> fromBytes(JSUint8Array bytes);
}

extension type _Result(JSObject _) implements JSObject {
  external int get code;
  external JSAny? get value;
}

extension type _Library(JSObject _) implements JSObject {
  external _Result prepare(
      JSUint8Array bytes, int format, int kind, JSUint8Array key);
  external _Result keyHint(JSUint8Array bytes, int format);
}

extension type _Decoder(JSObject _) implements JSObject {
  external _Info get info;
  external _Result decrypt(JSUint8Array bytes);
  external _Result decryptChunk(JSUint8Array bytes, int offset);
  external _Result dispose();
}

extension type _Info(JSObject _) implements JSObject {
  external int get audioOffset;
  external int get audioLength;
  external int get format;
  external int get audioFormat;
}

JSAny? _unwrap(_Result result) {
  if (result.code != 0) throw UmDecryptException(result.code);
  return result.value;
}

_Bridge _bridge() {
  final bridge = _installedBridge;
  if (bridge == null || bridge.abiVersion != 1) {
    throw StateError(
        'Load um-decrypt.mjs and call installDartBridge() before starting the Dart Web app');
  }
  return bridge;
}

/// Browser backend. Algorithms execute in WASM, not a second Dart implementation.
final class UmLibrary {
  final _Library _library;
  UmLibrary._(this._library);

  /// Synchronous native-style opening is unavailable on Web. Use load instead.
  factory UmLibrary.open(String path) =>
      throw UnsupportedError('Use await UmLibrary.load(wasmUrl) on Web');

  /// Fetches only the WASM program asset. Audio and keys remain in this browser.
  static Future<UmLibrary> load(String path) async {
    final result = await _bridge().load(path.toJS).toDart;
    return UmLibrary._(_Library(_unwrap(result) as JSObject));
  }

  /// Initialize from caller-supplied WASM bytes without fetching an asset.
  static Future<UmLibrary> fromWasmBytes(Uint8List wasm) async {
    final result = await _bridge().fromBytes(wasm.toJS).toDart;
    return UmLibrary._(_Library(_unwrap(result) as JSObject));
  }

  UmDecoder prepare(
    Uint8List container, {
    UmFormat format = UmFormat.auto,
    UmKey key = const UmKey.none(),
  }) {
    final result = _library.prepare(
        container.toJS, format.index, key.kind, key.bytes.toJS);
    final decoder = _Decoder(_unwrap(result) as JSObject);
    final info = decoder.info;
    return UmDecoder._(
        decoder,
        UmInfo.internal(
            UmFormat.values[info.format],
            UmAudioFormat.values[info.audioFormat - 1],
            info.audioOffset,
            info.audioLength));
  }

  UmDecryptedAudio decrypt(
    Uint8List container, {
    UmFormat format = UmFormat.auto,
    UmKey key = const UmKey.none(),
  }) {
    final decoder = prepare(container, format: format, key: key);
    try {
      return UmDecryptedAudio.internal(
          decoder.info, decoder.decrypt(container));
    } finally {
      decoder.dispose();
    }
  }

  String? keyHint(Uint8List container, {UmFormat format = UmFormat.auto}) {
    final hint = _unwrap(_library.keyHint(container.toJS, format.index));
    return hint == null ? null : (hint as JSString).toDart;
  }
}

/// Prepared browser decoder. Same payload-relative chunk semantics as native.
/// Dispose explicitly; the JS wrapper also has a GC finalizer fallback.
final class UmDecoder {
  final _Decoder _decoder;
  final UmInfo info;
  bool _disposed = false;
  UmDecoder._(this._decoder, this.info);

  void _ensureOpen() {
    if (_disposed) throw StateError('Decoder has been disposed');
  }

  Uint8List decryptChunk(Uint8List encrypted, {int offset = 0}) {
    _ensureOpen();
    if (offset < 0 ||
        offset > info.audioLength ||
        encrypted.length > info.audioLength - offset) {
      throw const UmDecryptException(9);
    }
    final result =
        _unwrap(_decoder.decryptChunk(encrypted.toJS, offset)) as JSUint8Array;
    return Uint8List.fromList(result.toDart);
  }

  void decryptChunkInPlace(Uint8List encrypted, {int offset = 0}) {
    final plain = decryptChunk(encrypted, offset: offset);
    encrypted.setAll(0, plain);
  }

  Uint8List decrypt(Uint8List container) {
    _ensureOpen();
    return Uint8List.fromList(
        (_unwrap(_decoder.decrypt(container.toJS)) as JSUint8Array).toDart);
  }

  void dispose() {
    if (_disposed) return;
    _unwrap(_decoder.dispose());
    _disposed = true;
  }
}
