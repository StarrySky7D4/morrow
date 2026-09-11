import 'dart:js_interop';
import 'dart:typed_data';
import 'package:morrow_core_client/web.dart';

@JS('morrowStoreRename')
external JSPromise<JSUint8Array> _rename(JSUint8Array input);
@JS('morrowFixture')
external JSPromise<JSUint8Array> _fixture(JSString name);
@JS('jsCodecProbeResult')
external set _result(JSString value);
Future<void> main() async {
  try {
    for (final revision in [
      BigInt.one,
      (BigInt.one << 53) + BigInt.one,
      (BigInt.one << 64) - BigInt.one,
    ]) {
      final input = RenameCommand(
        operationId: 'exact',
        cardId: 'card',
        expectedRevision: revision,
        title: '中文 🧭',
      );
      final output = RenameCommand.decode(input.encode());
      if (output.expectedRevision != revision || output.title != input.title)
        throw StateError('UInt64 mismatch');
    }
    for (final name in ['one', 'above-js', 'max', 'multisegment']) {
      final bytes = (await _fixture(name.toJS).toDart).toDart;
      final first = RenameCommand.decode(bytes);
      final second = RenameCommand.decode(first.encode());
      if (first.expectedRevision != second.expectedRevision ||
          first.title != second.title)
        throw StateError('Native vector mismatch');
    }
    var rejected = false;
    try {
      RenameCommand.decode(Uint8List.fromList([0, 1, 2]));
    } catch (_) {
      rejected = true;
    }
    if (!rejected) throw StateError('Invalid input accepted');
    final request = RenameCommand(
      operationId: 'js-edit',
      cardId: 'card',
      expectedRevision: BigInt.one,
      title: '普通 JS 已提交 🧭',
    );
    for (var attempt = 0; attempt < 2; attempt++) {
      final bytes = (await _rename(request.encode().toJS).toDart).toDart;
      final reply = RuntimeReply.decode(bytes, requestId: request.operationId);
      if (reply.kind != 'renamed' || reply.revision != BigInt.two)
        throw StateError('Persistent duplicate result mismatch');
      var mismatch = false;
      try {
        RuntimeReply.decode(bytes, requestId: 'another-request');
      } catch (_) {
        mismatch = true;
      }
      if (!mismatch) throw StateError('Response correlation accepted');
    }
    final read = ReadSummaryCommand(requestId: 'read-denied', cardId: 'card');
    final readReply = RuntimeReply.decode(
      (await _rename(read.encode().toJS).toDart).toDart,
      requestId: read.requestId,
    );
    if (readReply.kind != 'rejected' || readReply.failure != 'Denied')
      throw StateError('Read permission separation failed');
    _result =
        'PASS: ordinary Dart/JavaScript, exact UInt64, native vectors, OPFS commit and dedup.'
            .toJS;
  } catch (error) {
    _result = 'FAIL: $error'.toJS;
  }
}
