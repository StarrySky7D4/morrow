import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/generated/editor_draft_api.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;

typedef Change =
    void Function(
      wire.EnvelopeBuilder,
      wire.ArchiveMaintenanceReceiptBuilder,
      wire.ArchiveMaintenanceRequestBuilder,
    );
Uint8List envelope({Change? change}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.archiveMaintenance;
  out.cardId = 'card';
  out.draftId = 'draft';
  out.operation = 'maintenance';
  out.expectedGenerationBigInt = BigInt.from(2);
  final receipt = out.initArchiveMaintenance();
  receipt.auxiliaryCount = 2;
  receipt.auxiliaryBytesBigInt = BigInt.from(512);
  receipt.auxiliarySha256 = Uint8List.fromList(List.filled(32, 7));
  final request = receipt.initRequest();
  request.version = 1;
  request.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  request.cardId = 'card';
  request.draftId = 'draft';
  request.operation = 'maintenance';
  request.generationBigInt = BigInt.from(2);
  request.requestSha256 = Uint8List.fromList(List.filled(32, 1));
  request.archiveOperation = 'original-archive';
  request.archiveSha256 = Uint8List.fromList(List.filled(32, 2));
  change?.call(out, receipt, request);
  return message.serialize();
}

void main() {
  test('maintenance preserves separate archive and maintenance identities', () {
    final value = EditorDraftCodec.decodeEnvelope(envelope());
    expect(value.kind, EditorDraftResultKind.archiveMaintenance);
    final receipt = value.archiveMaintenance!;
    expect(receipt.operation, 'maintenance');
    expect(receipt.archiveOperation, 'original-archive');
    expect(receipt.archiveSha256, List.filled(32, 2));
    expect(receipt.auxiliaryCount, 2);
    expect(receipt.auxiliaryBytes, BigInt.from(512));
  });
  final controls = <String, Change>{
    'wrong outer operation': (out, receipt, request) =>
        out.operation = 'another-operation',
    'wrong outer generation': (out, receipt, request) =>
        out.expectedGenerationBigInt = BigInt.from(3),
    'zero counted journals': (out, receipt, request) =>
        receipt.auxiliaryCount = 0,
    'over production journal limit': (out, receipt, request) =>
        receipt.auxiliaryCount = 4097,
    'over production byte limit': (out, receipt, request) =>
        receipt.auxiliaryBytesBigInt = BigInt.from(64 * 1024 * 1024 + 1),
    'wrong archive digest length': (out, receipt, request) =>
        request.archiveSha256 = Uint8List(31),
    'wrong auxiliary digest length': (out, receipt, request) =>
        receipt.auxiliarySha256 = Uint8List(33),
    'changed nested draft': (out, receipt, request) =>
        request.draftId = 'another-draft',
    'changed schema digest': (out, receipt, request) =>
        request.digest = Uint8List(32),
    'reused archive operation': (out, receipt, request) =>
        request.archiveOperation = 'maintenance',
    'maintenance hidden in another result': (out, receipt, request) =>
        out.kind = wire.ResultKind.absent,
    'write-shaped inspection': (out, receipt, request) {
      out.operation = '';
      out.expectedGenerationBigInt = BigInt.zero;
    },
  };
  for (final control in controls.entries) {
    test('reject ${control.key}', () {
      expect(
        () => EditorDraftCodec.decodeEnvelope(envelope(change: control.value)),
        throwsFormatException,
      );
    });
  }
  test('inspection requires a repeated original receipt', () {
    final value = EditorDraftCodec.decodeEnvelope(
      envelope(
        change: (out, receipt, request) {
          out.operation = '';
          out.expectedGenerationBigInt = BigInt.zero;
          receipt.repeated = true;
        },
      ),
    );
    expect(value.archiveMaintenance!.repeated, true);
  });
}
