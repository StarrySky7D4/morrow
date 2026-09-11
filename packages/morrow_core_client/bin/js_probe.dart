import 'dart:js_interop';
import 'dart:typed_data';
import 'package:morrow_core_client/web.dart';

@JS('morrowStoreRename')
external JSPromise<JSUint8Array> _rename(JSUint8Array input);
@JS('morrowAttachmentControl')
external JSPromise<JSString> _attachmentControl(JSString action);
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
    final query = QueryOperationCommand(
      requestId: 'query-denied',
      cardId: 'card',
      operationId: 'js-edit',
    );
    final queryReply = RuntimeReply.decode(
      (await _rename(query.encode().toJS).toDart).toDart,
      requestId: query.requestId,
    );
    if (queryReply.failure != 'Denied')
      throw StateError('Query authorization missing');
    await _attachmentControl('attachment-seed'.toJS).toDart;
    Future<RuntimeReply> readPart({
      BigInt? offset,
      BigInt? revision,
      String attachment = 'file',
    }) async {
      final command = ReadAttachmentCommand(
        requestId: 'read-part',
        cardId: 'payload',
        attachmentId: attachment,
        expectedRevision: revision ?? BigInt.one,
        offset: offset ?? BigInt.zero,
      );
      return RuntimeReply.decode(
        (await _rename(command.encode().toJS).toDart).toDart,
        requestId: command.requestId,
      );
    }

    if ((await readPart()).failure != 'Denied')
      throw StateError('Attachment missing authorization');
    await _attachmentControl('attachment-grant'.toJS).toDart;
    if ((await readPart(attachment: 'other')).failure != 'Denied')
      throw StateError('Attachment grant too broad');
    final transfer = AttachmentTransferVerifier(
      cardId: 'payload',
      attachmentId: 'file',
      revision: BigInt.one,
    );
    var packets = 0;
    while (transfer.nextOffset < BigInt.from(100000)) {
      final reply = await readPart(offset: transfer.nextOffset);
      final part = reply.attachmentPart;
      if (reply.kind != 'attachmentChunk' || part == null)
        throw StateError('Missing attachment part: ${reply.failure}');
      if (part.totalLength != BigInt.from(100000))
        throw StateError('Attachment length mismatch');
      for (var index = 0; index < part.bytes.length; index++) {
        if (part.bytes[index] != (part.offset.toInt() + index) % 251)
          throw StateError('Raw attachment mismatch');
      }
      transfer.add(part);
      packets++;
    }
    transfer.finish();
    if (packets != 4) throw StateError('Expected four bounded packets');
    await _attachmentControl('attachment-revoke'.toJS).toDart;
    if ((await readPart()).failure != 'Denied')
      throw StateError('Revoked attachment leaked');
    await _attachmentControl('attachment-expire'.toJS).toDart;
    if ((await readPart()).failure != 'Denied')
      throw StateError('Expired attachment leaked');
    await _attachmentControl('attachment-grant'.toJS).toDart;
    final partial = AttachmentTransferVerifier(
      cardId: 'payload',
      attachmentId: 'file',
      revision: BigInt.one,
    );
    partial.add((await readPart()).attachmentPart!);
    await _attachmentControl('attachment-change'.toJS).toDart;
    if ((await readPart(offset: partial.nextOffset)).failure !=
        'RevisionConflict')
      throw StateError('Stale attachment revision accepted');
    var incomplete = false;
    try {
      partial.finish();
    } on FormatException {
      incomplete = true;
    }
    if (!incomplete ||
        (await readPart(revision: BigInt.two)).attachmentPart?.revision !=
            BigInt.two)
      throw StateError('Revision-bound transfer failed');
    _result =
        'PASS: ordinary Dart/JavaScript, exact UInt64, native vectors, OPFS commit/dedup, four attachment packets, whole-file SHA256, revoke/expiry and revision pinning.'
            .toJS;
  } catch (error) {
    _result = 'FAIL: $error'.toJS;
  }
}
