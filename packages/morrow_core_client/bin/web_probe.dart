import 'dart:js_interop';
import 'dart:typed_data';
import 'package:morrow_core_client/morrow_core_client.dart';

@JS('coreRoundTrip')
external JSPromise<JSUint8Array> exchange(JSUint8Array bytes);
@JS('coreFixture')
external JSPromise<JSUint8Array> fixture(JSString name);
@JS('coreProbeResult')
external set result(JSString text);
Future<void> main() async {
  try {
    for (final revision in [
      BigInt.one,
      (BigInt.one << 53) + BigInt.one,
      (BigInt.one << 64) - BigInt.one,
    ]) {
      final request = RenameCommand(
        operationId: 'web-op',
        cardId: 'legacy-123',
        expectedRevision: revision,
        title: '网页 🪷',
      );
      final output = (await exchange(request.encode().toJS).toDart).toDart;
      final decoded = RenameCommand.decode(output);
      if (decoded.expectedRevision != revision ||
          decoded.title != request.title ||
          decoded.operationId != request.operationId)
        throw StateError('Web message mismatch');
    }
    for (final name in ['one', 'above-js', 'max', 'multisegment']) {
      final input = (await fixture(name.toJS).toDart).toDart;
      final before = RenameCommand.decode(input);
      final after = RenameCommand.decode(
        (await exchange(input.toJS).toDart).toDart,
      );
      if (before.expectedRevision != after.expectedRevision ||
          after.title != '消息 🪷')
        throw StateError('Rust fixture mismatch');
    }
    var rejected = false;
    try {
      await exchange(Uint8List.fromList([0, 1, 2]).toJS).toDart;
    } catch (_) {
      rejected = true;
    }
    if (!rejected) throw StateError('Invalid Web message was accepted');
    result =
        'PASS: Dart/Wasm -> Worker -> Rust/Wasm -> Dart/Wasm, UInt64 and rejection.'
            .toJS;
  } catch (error) {
    result = 'FAIL: $error'.toJS;
  }
}
