import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';
import 'package:web/web.dart' as web;
import 'workbench_channel.dart';
import 'workbench_device_files.dart';
import '../attachments/attachment.dart';
import '../media/texture_source.dart';
import '../media/texture_storage_web.dart' as textures;

extension type _Message._(JSObject _) implements JSObject {
  external factory _Message({
    required String kind,
    String? name,
    bool? create,
    JSUint8Array? frame,
    String? card,
    String? assetId,
    String? mediaKind,
    web.Blob? blob,
  });
}
extension type _Reply(JSObject _) implements JSObject {
  external String get kind;
  external int? get code;
  external String? get message;
  external JSUint8Array? get frame;
  external String? get assetId;
  external int? get size;
  external web.Blob? get blob;
  external JSUint8Array? get digest;
}

final class BrowserWorkbenchChannel
    implements WorkbenchChannel, WorkbenchDeviceFiles {
  BrowserWorkbenchChannel._(this._worker) {
    _worker.onmessage = ((web.MessageEvent event) {
      try {
        _receive(_Reply(event.data as JSObject));
      } catch (error) {
        _abort(error);
      }
    }).toJS;
    _worker.onerror = ((web.Event event) {
      event.preventDefault();
      _abort(StateError('Device workbench worker failed'));
    }).toJS;
    _worker.onmessageerror = ((web.Event _) {
      _abort(const FormatException('Invalid device workbench message'));
    }).toJS;
  }
  final web.Worker _worker;
  final _ready = Completer<void>();
  final _exit = Completer<int>();
  final _output = StreamController<List<int>>();
  final _diagnostics = StreamController<List<int>>();
  bool _pending = false, _closing = false;
  Completer<_Reply>? _fileReply;
  final _previews = <TextureSource>[];

  /// The Worker loads the selected device identity. Private seed bytes never
  /// cross into this UI channel; create must be an explicit library intent.
  static Future<BrowserWorkbenchChannel> open({
    required String name,
    required bool create,
  }) async {
    BrowserWorkbenchChannel? channel;
    try {
      final worker = web.Worker(
        web.URL('workbench/host-worker.mjs', web.document.baseURI).href.toJS,
        web.WorkerOptions(type: 'module'),
      );
      channel = BrowserWorkbenchChannel._(worker);
      // Attach before postMessage, which may fail synchronously. Abort then
      // completes the same observed future instead of an unhandled error.
      final admission = channel._ready.future.timeout(
        const Duration(seconds: 60),
      );
      try {
        worker.postMessage(_Message(kind: 'open', name: name, create: create));
      } catch (error) {
        channel._abort(error);
      }
      await admission;
      return channel;
    } catch (error) {
      channel?._abort(error);
      rethrow;
    }
  }

  @override
  Stream<List<int>> get output => _output.stream;
  @override
  Stream<List<int>> get diagnostics => _diagnostics.stream;
  @override
  Future<int> get exitCode => _exit.future;
  void _receive(_Reply reply) {
    if (_exit.isCompleted) return;
    switch (reply.kind) {
      case 'ready':
        if (_ready.isCompleted) {
          throw const FormatException('Duplicate host admission');
        }
        _ready.complete();
      case 'reply':
        if (!_pending) throw const FormatException('Unordered host response');
        final frame = reply.frame?.toDart;
        if (frame == null || frame.isEmpty || frame.length > 256 * 1024) {
          throw const FormatException('Invalid response size');
        }
        final framed = Uint8List(frame.length + 4);
        ByteData.sublistView(framed).setUint32(0, frame.length, Endian.little);
        framed.setRange(4, framed.length, frame);
        frame.fillRange(0, frame.length, 0);
        _pending = false;
        _output.add(framed);
      case 'file-reply':
      case 'file-error':
        final pending = _fileReply;
        if (pending == null) {
          throw const FormatException('Unordered device file response');
        }
        _fileReply = null;
        if (reply.kind == 'file-error') {
          pending.completeError(
            StateError(reply.message ?? 'Device file operation failed'),
          );
        } else {
          pending.complete(reply);
        }
      case 'exit':
        final code = reply.code;
        if (code != 0 && code != 1 ||
            code == 0 && (!_closing || _pending || _fileReply != null)) {
          throw const FormatException('Invalid host exit');
        }
        final message = reply.message ?? '';
        if (message.isNotEmpty) _diagnostics.add(utf8.encode(message));
        _finish(
          code!,
          startupError: message.isEmpty ? null : StateError(message),
        );
      default:
        throw const FormatException('Unknown host response');
    }
  }

  void _finish(int code, {Object? startupError}) {
    if (_exit.isCompleted) return;
    if (!_ready.isCompleted) {
      _ready.completeError(
        startupError ?? StateError('Device library could not be opened'),
      );
    }
    _exit.complete(code);
    _fileReply?.completeError(
      startupError ?? StateError('Device file owner closed'),
    );
    _fileReply = null;
    unawaited(_diagnostics.close());
    unawaited(_output.close());
  }

  void _abort(Object error) {
    if (_exit.isCompleted) return;
    _worker.terminate();
    _diagnostics.add(utf8.encode(error.toString()));
    _finish(1, startupError: error);
  }

  @override
  Future<void> send(Uint8List frame) async {
    if (_exit.isCompleted ||
        _closing ||
        _pending ||
        _fileReply != null ||
        !_ready.isCompleted) {
      throw StateError('Device host is unavailable');
    }
    if (frame.isEmpty || frame.length > 128 * 1024) {
      throw const FormatException('Host request size');
    }
    _pending = true;
    try {
      _worker.postMessage(_Message(kind: 'request', frame: frame.toJS));
    } catch (error) {
      _abort(error);
      rethrow;
    }
  }

  Future<_Reply> _fileRequest(_Message message) async {
    if (_exit.isCompleted ||
        _closing ||
        _pending ||
        _fileReply != null ||
        !_ready.isCompleted) {
      throw StateError('Device file owner is unavailable');
    }
    final pending = _fileReply = Completer<_Reply>();
    final result = pending.future.timeout(
      const Duration(minutes: 2),
      onTimeout: () {
        final error = TimeoutException('Device file outcome is not confirmed');
        // A timeout is not proof that import rolled back. Ask the original
        // Worker to finish its accepted request and close, without replaying.
        unawaited(closeInput());
        throw error;
      },
    );
    try {
      _worker.postMessage(message);
    } catch (error) {
      _abort(error);
    }
    return result;
  }

  @override
  Future<IdeaAttachment> importDeviceFile(
    String card,
    IdeaAttachment selected,
  ) async {
    final blob = await textures.resolveBlob(selected.source);
    if (blob.size != selected.size || blob.size > IdeaAttachment.maxSize) {
      throw const FormatException('Selected attachment size changed');
    }
    final reply = await _fileRequest(
      _Message(
        kind: 'file-import',
        card: card,
        name: selected.source.name,
        mediaKind: selected.source.kind.name,
        blob: blob,
      ),
    );
    if (reply.assetId == null ||
        reply.assetId!.isEmpty ||
        reply.size != blob.size) {
      throw const FormatException('Invalid imported attachment receipt');
    }
    return IdeaAttachment(
      source: selected.source,
      size: reply.size!,
      pluginId: reply.assetId,
    );
  }

  @override
  Future<DeviceFilePreview> exportDeviceFile(
    String card,
    String asset,
    String name,
    TextureKind kind,
  ) async {
    final reply = await _fileRequest(
      _Message(kind: 'file-export', card: card, assetId: asset),
    );
    final blob = reply.blob;
    final digest = reply.digest?.toDart;
    if (blob == null ||
        reply.size != blob.size ||
        blob.size > IdeaAttachment.maxSize ||
        digest?.length != 32) {
      throw const FormatException('Invalid exported attachment receipt');
    }
    final source = textures.retainPreview(blob, name, kind);
    _previews.add(source);
    return DeviceFilePreview(
      source,
      BigInt.from(blob.size),
      digest!.map((v) => v.toRadixString(16).padLeft(2, '0')).join(),
    );
  }

  @override
  bool hasDevicePreview(TextureSource source) => textures.hasPreview(source);
  @override
  void releaseDevicePreview(TextureSource source) {
    _previews.remove(source);
    textures.releasePreview(source);
  }

  @override
  void releaseDevicePreviews() {
    for (final source in _previews) {
      textures.releasePreview(source);
    }
    _previews.clear();
  }

  @override
  Future<void> closeInput() async {
    if (_closing || _exit.isCompleted) return;
    // Queue shutdown behind a request whose reply may have timed out. The
    // Worker must finish/reject that accepted request before releasing storage.
    _closing = true;
    try {
      _worker.postMessage(_Message(kind: 'close'));
    } catch (error) {
      _abort(error);
      rethrow;
    }
  }
}
