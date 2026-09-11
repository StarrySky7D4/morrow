import 'dart:ffi';
import 'dart:convert';
import 'dart:typed_data';
import 'src/codec.dart';

/// Same-process trusted client only. All native buffers have one synchronous owner.
class NativeCoreBridge {
  NativeCoreBridge(String path) : this.library(DynamicLibrary.open(path));
  NativeCoreBridge.library(DynamicLibrary library) {
    _new = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_buffer_new',
    );
    _ptr = library
        .lookupFunction<
          Pointer<Uint8> Function(Uint32),
          Pointer<Uint8> Function(int)
        >('morrow_buffer_ptr');
    _len = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_buffer_len',
    );
    _process = library
        .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
          'morrow_buffer_process',
        );
    _status = library
        .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
          'morrow_buffer_status',
        );
    _free = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_buffer_free',
    );
    _live = library.lookupFunction<Uint32 Function(), int Function()>(
      'morrow_buffer_live',
    );
  }
  late final int Function(int) _new, _len, _process, _status, _free;
  late final Pointer<Uint8> Function(int) _ptr;
  late final int Function() _live;
  int get liveBuffers => _live();
  Uint8List roundTrip(Uint8List bytes) {
    if (bytes.isEmpty || bytes.length > maxMessageBytes)
      throw const FormatException('Message length');
    final input = _new(bytes.length);
    if (input == 0) throw StateError('Core buffer limit');
    var output = 0;
    try {
      final inputPointer = _ptr(input);
      if (inputPointer == nullptr) throw StateError('Invalid input buffer');
      inputPointer.asTypedList(bytes.length).setAll(0, bytes);
      output = _process(input);
      if (output == 0)
        throw FormatException('Core rejected message: ${_status(input)}');
      final len = _len(output), pointer = _ptr(output);
      if (len == 0 || len > maxMessageBytes || pointer == nullptr)
        throw StateError('Invalid output buffer');
      return Uint8List.fromList(pointer.asTypedList(len));
    } finally {
      if (output != 0) _free(output);
      _free(input);
    }
  }
}

class NativeHostException implements Exception {
  NativeHostException(this.operation, this.status);
  final String operation;
  final int status;
  @override
  String toString() =>
      'Native host $operation failed (status $status); query an uncertain write before retrying.';
}

enum NativeCapability { rename, readSummary, queryOperation }

/// Trusted, synchronous embedding host. Close explicitly; never expose control methods to plugins.
class NativeHostSession {
  NativeHostSession(String libraryPath, String databasePath) {
    final library = DynamicLibrary.open(libraryPath);
    _bridge = NativeCoreBridge.library(library);
    _open = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_host_open',
    );
    _close = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_host_close',
    );
    _connect = library
        .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
          'morrow_host_connect',
        );
    _disconnect = library
        .lookupFunction<
          Uint32 Function(Uint32, Uint32),
          int Function(int, int)
        >('morrow_host_disconnect');
    _grant = library
        .lookupFunction<
          Uint32 Function(Uint32, Uint32, Uint32, Uint32, Uint32),
          int Function(int, int, int, int, int)
        >('morrow_host_grant');
    _revoke = library
        .lookupFunction<
          Uint32 Function(Uint32, Uint32, Uint32, Uint32),
          int Function(int, int, int, int)
        >('morrow_host_revoke');
    _dispatch = library
        .lookupFunction<
          Uint32 Function(Uint32, Uint32, Uint32),
          int Function(int, int, int)
        >('morrow_host_dispatch');
    _live = library.lookupFunction<Uint32 Function(), int Function()>(
      'morrow_host_live',
    );
    _host = _control(
      'open',
      Uint8List.fromList(utf8.encode(databasePath)),
      _open,
    );
  }
  late final NativeCoreBridge _bridge;
  late final int Function(int) _open, _close, _connect;
  late final int Function(int, int) _disconnect;
  late final int Function(int, int, int, int, int) _grant;
  late final int Function(int, int, int, int) _revoke;
  late final int Function(int, int, int) _dispatch;
  late final int Function() _live;
  int _host = 0;
  int get liveHosts => _live();
  int get liveBuffers => _bridge.liveBuffers;
  void _ensureOpen() {
    if (_host == 0) throw StateError('Host closed');
  }

  int _control(String operation, Uint8List bytes, int Function(int) action) {
    if (bytes.isEmpty || bytes.length > maxMessageBytes)
      throw const FormatException('Input length');
    final input = _bridge._new(bytes.length);
    if (input == 0) throw StateError('Core buffer limit');
    try {
      final pointer = _bridge._ptr(input);
      if (pointer == nullptr) throw StateError('Invalid input buffer');
      pointer.asTypedList(bytes.length).setAll(0, bytes);
      final result = action(input);
      if (result == 0)
        throw NativeHostException(operation, _bridge._status(input));
      return result;
    } finally {
      _bridge._free(input);
    }
  }

  NativeHostConnection connect() {
    _ensureOpen();
    final id = _connect(_host);
    if (id == 0) throw StateError('Connection unavailable');
    return NativeHostConnection._(this, id);
  }

  void close() {
    if (_host != 0) {
      _close(_host);
      _host = 0;
    }
  }
}

class NativeHostConnection {
  NativeHostConnection._(this._owner, this._id);
  final NativeHostSession _owner;
  int _id;
  void _ensureOpen() {
    _owner._ensureOpen();
    if (_id == 0) throw StateError('Connection closed');
  }

  void grant(
    NativeCapability capability,
    String cardId, {
    Duration ttl = const Duration(minutes: 1),
  }) {
    _ensureOpen();
    final milliseconds = ttl.inMilliseconds;
    if (milliseconds <= 0 || milliseconds > 0xffffffff)
      throw ArgumentError('TTL limit');
    _owner._control(
      'grant',
      Uint8List.fromList(utf8.encode(cardId)),
      (input) => _owner._grant(
        _owner._host,
        _id,
        capability.index + 1,
        input,
        milliseconds,
      ),
    );
  }

  void revoke(NativeCapability capability, String cardId) {
    _ensureOpen();
    _owner._control(
      'revoke',
      Uint8List.fromList(utf8.encode(cardId)),
      (input) => _owner._revoke(_owner._host, _id, capability.index + 1, input),
    );
  }

  Uint8List dispatch(Uint8List bytes) {
    _ensureOpen();
    var output = 0;
    try {
      output = _owner._control(
        'dispatch',
        bytes,
        (input) => _owner._dispatch(_owner._host, _id, input),
      );
      final length = _owner._bridge._len(output),
          pointer = _owner._bridge._ptr(output);
      if (length == 0 || length > maxMessageBytes || pointer == nullptr)
        throw StateError('Invalid response buffer');
      return Uint8List.fromList(pointer.asTypedList(length));
    } finally {
      if (output != 0) _owner._bridge._free(output);
    }
  }

  void close() {
    if (_id != 0) {
      if (_owner._host != 0) _owner._disconnect(_owner._host, _id);
      _id = 0;
    }
  }
}
