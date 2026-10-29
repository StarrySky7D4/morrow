import 'dart:async';
import 'dart:typed_data';
import 'package:test/test.dart';
import '../../../lib/plugins/blob_upload_models.dart';
import '../../../lib/plugins/blob_upload_session.dart';
import '../../../lib/plugins/io_task_models.dart';
import 'blob_upload_metadata_peer.dart';
import 'blob_upload_session_test.dart' show Original;
import 'blob_upload_client_test.dart' show request, id;

final class RetainedAck implements WorkbenchIoTaskControl {
  RetainedAck(this.original);
  final Original original;
  int attempts = 0;
  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    attempts++;
    original.recovery = true;
    throw StateError('original acknowledgement metadata not committed');
  }

  @override
  Future<IoTaskSnapshot> ioStatus() => original.ioStatus();
  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) => original.pollIo(key);
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) => original.cancelIo(key);
  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) => original.repairIo(key);
  @override
  Future<IoTaskRead> readIo(Uint8List key) => original.readIo(key);
  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request) =>
      original.startHttp(request);
}

Future<(Original, MetadataPeer, BlobUploadSession, Object)> setup({
  bool unknown = false,
  WorkbenchIoTaskControl Function(Original)? control,
}) async {
  final original = Original();
  final metadata = MetadataPeer(original)..unknown = unknown;
  final session = BlobUploadSession.forBackend(
    metadata,
    control?.call(original) ?? original,
  );
  final view = session.attachView();
  await session.refresh(view);
  await session.start(view, request());
  original.ready = true;
  original.recovery = unknown;
  await session.refresh(view);
  await session.read(view);
  original.recovery = true;
  await session.refresh(view);
  metadata.onComplete = (_) async {
    original.recovery = unknown;
    return BlobUploadMetadataCompletion(original.io, original.snapshot);
  };
  return (original, metadata, session, view);
}

void main() {
  test(
    'read reconciliation invalidates the view of an earlier metadata attempt',
    () async {
      final (_, peer, session, view) = await setup();
      await session.reviewMetadata(view);
      expect(session.canCompleteMetadata, isTrue);
      await session.repair(view);
      expect(session.metadataReview, isNull);
      expect(await session.completeMetadata(view), isFalse);
      expect(peer.calls, ['review']);
    },
  );
  test(
    'starting acknowledgement invalidates the previous observation review',
    () async {
      final (original, peer, session, view) = await setup(
        control: RetainedAck.new,
      );
      await session.reviewMetadata(view);
      original.recovery = false;
      await session.refresh(view);
      expect(await session.acknowledge(view), isFalse);
      expect(session.metadataReview, isNull);
      expect(await session.completeMetadata(view), isFalse);
      expect(peer.calls, ['review']);
    },
  );
  test(
    'metadata needs explicit review and completion; upload and result are not repeated',
    () async {
      final (original, peer, session, view) = await setup();
      expect(session.canAcknowledge, isFalse);
      expect(await session.completeMetadata(view), isFalse);
      expect(peer.calls, isEmpty);
      expect(await session.reviewMetadata(view), isTrue);
      expect(peer.calls, ['review']);
      expect(session.canAcknowledge, isFalse);
      expect(await session.completeMetadata(view), isTrue);
      expect(session.canAcknowledge, isTrue);
      expect(session.history, isEmpty);
      expect(original.calls.where((c) => c == 'start').length, 1);
      expect(original.calls.where((c) => c == 'read').length, 1);
      expect(await session.acknowledge(view), isTrue);
      expect(session.history.length, 1);
    },
  );
  test(
    'metadata cannot clear an original unknown result or grant acknowledgement',
    () async {
      final (_, peer, session, view) = await setup(unknown: true);
      await session.reviewMetadata(view);
      expect(session.metadataReview!.originalUnknown, isTrue);
      await session.completeMetadata(view);
      expect(session.result!.unknown, isTrue);
      expect(session.canAcknowledge, isFalse);
      expect(session.canStart, isFalse);
      expect(peer.calls, ['review', 'complete']);
    },
  );
  test(
    'lost completion reply retains original identity and never sends completion again',
    () async {
      final (original, peer, session, view) = await setup();
      peer.loseCompletion = true;
      await session.reviewMetadata(view);
      expect(await session.completeMetadata(view), isFalse);
      expect(await session.completeMetadata(view), isFalse);
      expect(await session.reviewMetadata(view), isFalse);
      expect(session.originalKey, id(3));
      expect(session.canStart, isFalse);
      expect(session.history, isEmpty);
      expect(peer.calls.where((c) => c == 'complete').length, 1);
      expect(original.calls.where((c) => c == 'start').length, 1);
    },
  );
  test(
    'detached review and changed task identity cannot approve a completion',
    () async {
      final (_, peer, session, view) = await setup();
      peer.reviewWait = Completer<void>();
      final waiting = session.reviewMetadata(view);
      await session.detachView(view, cancelOriginal: false);
      peer.reviewWait!.complete();
      await waiting;
      expect(session.metadataReview, isNull);
      expect(await session.completeMetadata(view), isFalse);
      final current = session.attachView();
      peer.wrongKey = true;
      expect(await session.reviewMetadata(current), isFalse);
      expect(await session.completeMetadata(current), isFalse);
      expect(peer.calls.contains('complete'), isFalse);
    },
  );
  test('double click while completion waits cannot write twice', () async {
    final (_, peer, session, view) = await setup();
    peer.completionWait = Completer<void>();
    await session.reviewMetadata(view);
    final pending = session.completeMetadata(view);
    expect(await session.completeMetadata(view), isFalse);
    peer.completionWait!.complete();
    expect(await pending, isTrue);
    expect(peer.calls.where((c) => c == 'complete').length, 1);
  });
  test(
    'interrupted acknowledgement finishes from its review without sending acknowledgement twice',
    () async {
      late RetainedAck control;
      final (original, peer, session, view) = await setup(
        control: (o) => control = RetainedAck(o),
      );
      original.recovery = false;
      await session.refresh(view);
      expect(await session.acknowledge(view), isFalse);
      peer.phase = BlobUploadMetadataPhase.acknowledgement;
      peer.onComplete = (_) async {
        original.active = original.ready = original.delivered =
            original.recovery = false;
        return BlobUploadMetadataCompletion(original.io, null);
      };
      expect(session.acknowledgementUncertain, isTrue);
      expect(await session.reviewMetadata(view), isTrue);
      expect(await session.completeMetadata(view), isTrue);
      expect(control.attempts, 1);
      expect(session.acknowledgementUncertain, isFalse);
      expect(session.canStart, isTrue);
      expect(session.history.length, 1);
      expect(original.calls.where((c) => c == 'start').length, 1);
    },
  );
}
