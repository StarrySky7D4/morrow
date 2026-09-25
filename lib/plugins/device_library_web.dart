import 'dart:async';
import 'dart:js_interop';
import 'package:web/web.dart' as web;

enum DeviceLibraryState { empty, prepared, ready, orphaned }

extension type _Request._(JSObject _) implements JSObject {
  external factory _Request({required String kind, required String name});
}
extension type _Response(JSObject _) implements JSObject {
  external String get kind;
  external String? get state;
  external String? get message;
}

Future<DeviceLibraryState> inspectDeviceLibrary() async {
  final worker = web.Worker(
    web.URL('workbench/library-worker.mjs', web.document.baseURI).href.toJS,
    web.WorkerOptions(type: 'module'),
  );
  final reply = Completer<DeviceLibraryState>();
  void fail(Object error) {
    if (!reply.isCompleted) reply.completeError(error);
  }

  worker.onmessage = ((web.MessageEvent event) {
    if (reply.isCompleted) return;
    try {
      final message = _Response(event.data as JSObject);
      if (message.kind != 'state') {
        throw StateError(
          message.message ?? 'Local workspace inspection failed',
        );
      }
      reply.complete(DeviceLibraryState.values.byName(message.state ?? ''));
    } catch (error) {
      fail(error);
    }
  }).toJS;
  worker.onerror = ((web.Event event) {
    event.preventDefault();
    fail(StateError('Local workspace inspection failed'));
  }).toJS;
  worker.onmessageerror = ((web.Event event) {
    fail(const FormatException('Invalid workspace inspection response'));
  }).toJS;
  final result = reply.future.timeout(const Duration(seconds: 30));
  try {
    try {
      worker.postMessage(_Request(kind: 'inspect', name: 'main'));
    } catch (error) {
      fail(error);
    }
    return await result;
  } finally {
    worker.terminate();
  }
}
