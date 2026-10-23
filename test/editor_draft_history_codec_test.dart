import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/generated/editor_draft_api.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;

Uint8List history([
  void Function(wire.EnvelopeBuilder, wire.HistorySummaryBuilder)? change,
]) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.history;
  out.cardId = '';
  out.draftId = '';
  out.operation = '';
  out.requestCursor = '';
  out.nextCursor = '';
  out.requestLimit = 32;
  out.unarchivedCount = 256;
  out.archivedCount = 1;
  out.archivedBytes = 1000;
  final row = out.initHistory(1)[0];
  row.cardId = 'card';
  row.draftId = 'draft';
  row.generation = 2;
  row.requestSha256 = Uint8List(32);
  row.archiveOperation = 'archived-operation';
  row.title = '';
  row.preview = 'Visible draft preview';
  row.cursor = 'morrow-host-editor-draft-${'a' * 64}';
  change?.call(out, row);
  return message.serialize();
}

Uint8List archived([
  void Function(wire.EnvelopeBuilder, wire.ArchiveReceiptBuilder)? change,
]) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.archive;
  out.cardId = 'card';
  out.draftId = 'draft';
  out.operation = 'archive-op';
  out.expectedGeneration = 2;
  final record = out.initArchive();
  record.cardId = 'card';
  record.draftId = 'draft';
  record.generation = 2;
  record.requestSha256 = Uint8List(32);
  record.slotSha256 = Uint8List(32);
  record.archiveVersion = 2;
  record.archiveSha256 = Uint8List(32);
  record.operation = 'archive-op';
  record.repeated = false;
  change?.call(out, record);
  return message.serialize();
}

void main() {
  test(
    'history retains exact identity, generation, quota and ordered cursor',
    () {
      final page = EditorDraftCodec.decodeEnvelope(history()).history!;
      expect((page.unarchivedCount, page.archivedCount), (256, 1));
      expect(page.rows.single.draftId, 'draft');
      expect(page.rows.single.title, '');
      expect(page.rows.single.preview, 'Visible draft preview');
      expect(page.rows.single.generation, BigInt.two);
      expect(page.rows.single.archiveOperation, 'archived-operation');
      expect(page.requestCursor, '');
      expect(page.nextCursor, '');
    },
  );
  test(
    'history rejects malformed pages, excess capacity and mixed result types',
    () {
      for (final change
          in <void Function(wire.EnvelopeBuilder, wire.HistorySummaryBuilder)>[
            (v, _) => v.requestLimit = 0,
            (v, _) => v.requestLimit = 33,
            (v, _) => v.unarchivedCount = 257,
            (v, _) => v.archivedCount = 4097,
            (v, _) => v.archivedBytes = 64 * 1024 * 1024 + 1,
            (v, _) => v.nextCursor = 'morrow-host-editor-draft-${'b' * 64}',
            (v, _) => v.requestCursor = 'morrow-host-editor-draft-${'a' * 64}',
            (_, row) => row.requestSha256 = Uint8List(31),
            (_, row) => row.generation = 0,
            (_, row) => row.cursor = 'foreign-cursor',
            (_, row) => row.title = 'x' * 1025,
            (_, row) => row.preview = 'x' * 1025,
            (v, _) => v.cardId = 'unexpected',
            (v, _) {
              v.initArchive();
            },
            (v, _) {
              v.initRecord();
            },
          ]) {
        expect(
          () => EditorDraftCodec.decodeEnvelope(history(change)),
          throwsFormatException,
        );
      }
    },
  );
  test('archive completion remains bound to original target and operation', () {
    final receipt = EditorDraftCodec.decodeEnvelope(archived()).archive!;
    expect(
      (receipt.cardId, receipt.draftId, receipt.operation),
      ('card', 'draft', 'archive-op'),
    );
    for (final change
        in <void Function(wire.EnvelopeBuilder, wire.ArchiveReceiptBuilder)>[
          (_, row) => row.cardId = 'other',
          (_, row) => row.draftId = 'other',
          (_, row) => row.generation = 3,
          (_, row) => row.operation = 'other',
          (_, row) => row.slotSha256 = Uint8List(31),
          (_, row) => row.archiveVersion = 0,
          (_, row) => row.archiveVersion = 3,
          (_, row) => row.archiveSha256 = Uint8List(31),
          (_, row) => row.legacyAuxiliaryCount = 1,
          (v, _) => v.operation = 'changed-request',
          (v, _) => v.expectedGeneration = 0,
          (v, _) {
            v.initHistory(0);
          },
          (v, _) => v.archivedCount = 1,
        ]) {
      expect(
        () => EditorDraftCodec.decodeEnvelope(archived(change)),
        throwsFormatException,
      );
    }
  });
  test('inspection is read only and never substitutes a fresh completion', () {
    final bytes = archived((v, row) {
      v.operation = '';
      v.expectedGeneration = 0;
      row.repeated = true;
    });
    expect(EditorDraftCodec.decodeEnvelope(bytes).archive!.repeated, isTrue);
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        archived((v, _) {
          v.operation = '';
          v.expectedGeneration = 0;
        }),
      ),
      throwsFormatException,
    );
  });
}
