import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';
import 'bindings.dart';
import 'types.dart';
export 'types.dart';

void _check(int code) {
  if (code != 0) throw UmDecryptException(code);
}

/// A loaded native library. Create one in each isolate that performs decryption.
/// Methods are synchronous; applications can use a worker isolate for large files.
final class UmLibrary {
  final Bindings _bindings;
  UmLibrary.open(String path) : _bindings = Bindings(DynamicLibrary.open(path));

  /// Cross-platform initialization. Web loads a WASM asset asynchronously;
  /// native platforms open the supplied shared-library path.
  static Future<UmLibrary> load(String path) async => UmLibrary.open(path);

  /// Available on the Web backend. Native callers should use load/open with
  /// the platform shared-library path.
  static Future<UmLibrary> fromWasmBytes(Uint8List wasm) async =>
      throw UnsupportedError(
          'fromWasmBytes is available on Web; use load on native platforms');

  /// For example, use DynamicLibrary.process() for an application that statically
  /// links and retains the C symbols on iOS. Packaging is the application's job.
  UmLibrary.fromDynamicLibrary(DynamicLibrary library)
      : _bindings = Bindings(library);

  /// Borrows a full container through a temporary native copy, then releases it.
  /// Prepared state retains only keys/header bytes. Maximum container: 512 MiB.
  UmDecoder prepare(
    Uint8List container, {
    UmFormat format = UmFormat.auto,
    UmKey key = const UmKey.none(),
  }) {
    final buffers = _Buffers(_bindings);
    int handle = 0;
    try {
      final data = buffers.copy(container);
      final keyBytes = key.bytes;
      final keyData = buffers.copy(keyBytes);
      final out = buffers.alloc(sizeOf<Uint64>()).cast<Uint64>();
      _check(_bindings.create(data, container.length, format.index, keyData,
          keyBytes.length, key.kind, out));
      handle = out.value;
      final nativeInfo = buffers.alloc(sizeOf<NativeInfo>()).cast<NativeInfo>();
      _check(_bindings.info(handle, nativeInfo));
      final info = nativeInfo.ref;
      return UmDecoder._(
          _bindings,
          handle,
          container.length,
          UmInfo.internal(
              UmFormat.values[info.format],
              UmAudioFormat.values[info.audioFormat - 1],
              info.audioOffset,
              info.audioLength));
    } catch (_) {
      if (handle != 0) _bindings.dispose(handle);
      rethrow;
    } finally {
      buffers.dispose();
    }
  }

  /// Convenience operation. The returned bytes belong to Dart, with no native
  /// handle or native allocation left for the caller to dispose.
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

  /// Returns the untrusted KGM v5 audio hash or QMC musicex filename for
  /// application-managed key lookup. Null means no identifier is present.
  String? keyHint(Uint8List container, {UmFormat format = UmFormat.auto}) {
    final buffers = _Buffers(_bindings);
    try {
      final data = buffers.copy(container);
      final count = buffers.alloc(sizeOf<UintPtr>()).cast<UintPtr>();
      _check(_bindings.hint(
          data, container.length, format.index, nullptr, 0, count));
      if (count.value == 0) return null;
      final length = count.value;
      final output = buffers.alloc(length);
      _check(_bindings.hint(
          data, container.length, format.index, output, length, count));
      return utf8.decode(output.asTypedList(length));
    } finally {
      buffers.dispose();
    }
  }
}

final class _Cleanup {
  final Bindings bindings;
  final int handle;
  const _Cleanup(this.bindings, this.handle);
}

/// Prepared decryption state. Dispose explicitly in a finally block. A GC
/// finalizer is a fallback, not a guarantee of prompt resource release.
final class UmDecoder {
  static final _finalizer = Finalizer<_Cleanup>((state) {
    state.bindings.dispose(state.handle);
  });
  final Bindings _bindings;
  final int _handle;
  final int _sourceLength;
  final UmInfo info;
  bool _disposed = false;

  UmDecoder._(this._bindings, this._handle, this._sourceLength, this.info) {
    _finalizer.attach(this, _Cleanup(_bindings, _handle), detach: this);
  }

  void _ensureOpen() {
    if (_disposed) throw StateError('Decoder has been disposed');
  }

  /// Returns a decrypted copy of original encrypted payload bytes. Offset is
  /// relative to payload start, NOT the container. Arbitrary chunk order works.
  Uint8List decryptChunk(Uint8List encrypted, {int offset = 0}) {
    _ensureOpen();
    if (offset < 0 ||
        offset > info.audioLength ||
        encrypted.length > info.audioLength - offset) {
      throw const UmDecryptException(9);
    }
    final buffers = _Buffers(_bindings);
    try {
      final data = buffers.copy(encrypted);
      _check(_bindings.decrypt(_handle, offset, data, encrypted.length));
      return encrypted.isEmpty
          ? Uint8List(0)
          : Uint8List.fromList(data.asTypedList(encrypted.length));
    } finally {
      buffers.dispose();
    }
  }

  /// Replaces the supplied bytes only after successful native decryption.
  void decryptChunkInPlace(Uint8List encrypted, {int offset = 0}) {
    final plain = decryptChunk(encrypted, offset: offset);
    encrypted.setAll(0, plain);
  }

  /// The container must be the one used during prepare. Only length is checked;
  /// these legacy music formats do not provide cryptographic authentication.
  Uint8List decrypt(Uint8List container) {
    _ensureOpen();
    if (container.length != _sourceLength) throw const UmDecryptException(1);
    return decryptChunk(Uint8List.sublistView(
        container, info.audioOffset, info.audioOffset + info.audioLength));
  }

  void dispose() {
    if (_disposed) return;
    _check(_bindings.dispose(_handle));
    _disposed = true;
    _finalizer.detach(this);
  }
}

final class _Buffers {
  final Bindings bindings;
  final _allocations = <(Pointer<Uint8>, int)>[];
  _Buffers(this.bindings);

  Pointer<Uint8> alloc(int length) {
    if (length == 0) return nullptr;
    if (length < 0 || length > 512 * 1024 * 1024) {
      throw const UmDecryptException(10);
    }
    final pointer = bindings.alloc(length);
    if (pointer == nullptr) throw const UmDecryptException(10);
    _allocations.add((pointer, length));
    return pointer;
  }

  Pointer<Uint8> copy(Uint8List bytes) {
    final pointer = alloc(bytes.length);
    if (bytes.isNotEmpty) pointer.asTypedList(bytes.length).setAll(0, bytes);
    return pointer;
  }

  void dispose() {
    for (final (pointer, length) in _allocations.reversed) {
      bindings.free(pointer, length);
    }
    _allocations.clear();
  }
}
