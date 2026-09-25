import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_import_codec.dart';
import 'package:morrow_studio/plugins/generated/editor_draft_staging_api.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as identity;

EditorDraftImportRequest proposal({String operation = 'import-original'}) =>
    EditorDraftImportRequest(
      cardId: 'card-one',
      draftId: 'draft-one',
      operation: operation,
      expectedGeneration: BigInt.one,
      name: 'selected.png',
      kind: 'image',
      bytes: BigInt.from(4),
      sha256: List<int>.filled(32, 7),
    );

void writeRequest(wire.ImportRequestBuilder out, EditorDraftImportRequest r) {
  out.version = 1;
  out.digest = Uint8List.fromList(identity.editor_draft_staging_apiDigest);
  out.cardId = r.cardId;
  out.draftId = r.draftId;
  out.operation = r.operation;
  out.expectedGenerationBigInt = r.expectedGeneration;
  out.name = r.name;
  out.kind = r.kind;
  out.bytesBigInt = r.bytes;
  out.sha256 = Uint8List.fromList(r.sha256);
}

Uint8List envelope(
  EditorDraftImportRequest r, {
  String? nestedOperation,
  bool retained = true,
  bool? active,
  bool badDigest = false,
  wire.Phase phase = wire.Phase.ready,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(
    badDigest
        ? List<int>.filled(32, 0)
        : identity.editor_draft_staging_apiDigest,
  );
  out.kind = wire.ResultKind.record;
  out.cardId = r.cardId;
  out.draftId = r.draftId;
  out.operation = r.operation;
  out.expectedGenerationBigInt = r.expectedGeneration;
  out.importOperation = r.operation;
  out.currentGeneration = 1;
  out.mainActive = true;
  out.stagingRevision = 2;
  final record = out.initRecord();
  writeRequest(
    record.initRequest(),
    proposal(operation: nestedOperation ?? r.operation),
  );
  record.assetId = 'asset-one';
  record.phase = phase;
  record.currentActive = active ?? phase == wire.Phase.ready;
  record.bytesRetained = retained;
  record.stagingRevision = 2;
  record.currentGeneration = 1;
  record.mainActive = true;
  return message.serialize();
}

void writeDecision(
  wire.DecisionBuilder out,
  EditorDraftImportRequest request, {
  String operation = 'abandon-original',
  wire.DecisionStatus status = wire.DecisionStatus.pending,
  int committedRevision = 0,
  int? decisionRevision,
}) {
  writeRequest(out.initRequest(), request);
  out.operation = operation;
  out.expectedGeneration = 1;
  out.status = status;
  out.currentGeneration = 1;
  out.mainActive = true;
  out.stagingRevision = 2;
  out.decisionRevision =
      decisionRevision ?? (status == wire.DecisionStatus.cancelled ? 2 : 1);
  out.committedRevision = committedRevision;
}

Uint8List decisionEnvelope(
  EditorDraftImportRequest request, {
  bool list = false,
  int count = 1,
  bool wrongOriginal = false,
  wire.DecisionStatus status = wire.DecisionStatus.pending,
  int committedRevision = 0,
  int? decisionRevision,
  int limit = 2,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(identity.editor_draft_staging_apiDigest);
  out.kind = list ? wire.ResultKind.decisions : wire.ResultKind.decision;
  out.cardId = request.cardId;
  out.draftId = request.draftId;
  out.operation = list ? '' : 'abandon-original';
  out.expectedGeneration = list ? 0 : 1;
  out.importOperation = list ? '' : request.operation;
  out.currentGeneration = 1;
  out.mainActive = true;
  out.stagingRevision = 2;
  if (list) {
    out.requestCursor = 'cursor-one';
    out.requestLimit = limit;
    out.nextCursor = count == 0 ? '' : 'cursor-two';
    final entries = out.initDecisions(count);
    for (var i = 0; i < count; i++) {
      writeDecision(
        entries[i],
        request,
        operation: i == 0 ? 'abandon-original' : 'abandon-next',
      );
    }
  } else {
    writeDecision(
      out.initDecision(),
      wrongOriginal ? proposal(operation: 'foreign-import') : request,
      status: status,
      committedRevision: committedRevision,
      decisionRevision: decisionRevision,
    );
  }
  return message.serialize();
}

Uint8List scopeEnvelope({
  String cursor = 'cursor-one',
  String nextCursor = 'cursor-two',
  int limit = 2,
  int count = 1,
  String card = '',
  bool active = false,
  bool duplicate = false,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(identity.editor_draft_staging_apiDigest);
  out.kind = wire.ResultKind.scopes;
  out.cardId = card;
  out.draftId = '';
  out.operation = '';
  out.importOperation = '';
  out.currentGeneration = 0;
  out.stagingRevision = 0;
  out.mainActive = active;
  out.requestCursor = cursor;
  out.requestLimit = limit;
  out.nextCursor = nextCursor;
  final entries = out.initScopes(count);
  for (var i = 0; i < count; i++) {
    entries[i].cardId = 'card-one';
    entries[i].draftId = duplicate || i == 0 ? 'draft-one' : 'draft-two';
  }
  return message.serialize();
}

void main() {
  test('request SHA is deeply frozen and u64 remains exact', () {
    final hash = List<int>.filled(32, 7);
    final request = EditorDraftImportRequest(
      cardId: 'card-one',
      draftId: 'draft-one',
      operation: 'import-original',
      expectedGeneration: BigInt.parse('9007199254740993'),
      name: 'x',
      kind: 'file',
      bytes: BigInt.from(4),
      sha256: hash,
    );
    hash[0] = 0;
    expect(request.sha256.first, 7);
    expect(() => request.sha256[0] = 1, throwsUnsupportedError);
    final encoded = EditorDraftImportCodec.encodeRequest(request);
    final wireRequest = MessageReader.deserialize(
      encoded,
    ).getRoot(wire.importRequestFactory);
    expect(
      wireRequest.expectedGenerationBigInt,
      BigInt.parse('9007199254740993'),
    );
    expect(wireRequest.sha256!.first, 7);
  });

  test(
    'invalid identity, surrogate, budget, hash, and reserved op reject locally',
    () {
      EditorDraftImportRequest alter({
        String card = 'card-one',
        String op = 'import-original',
        String name = 'x',
        BigInt? bytes,
        List<int>? digest,
      }) => EditorDraftImportRequest(
        cardId: card,
        draftId: 'draft-one',
        operation: op,
        expectedGeneration: BigInt.one,
        name: name,
        kind: 'file',
        bytes: bytes ?? BigInt.one,
        sha256: digest ?? List<int>.filled(32, 1),
      );
      for (final value in [
        alter(card: '../card'),
        alter(name: '\ud800'),
        alter(name: ''),
        alter(op: 'draft-import-stage-forbidden'),
        alter(bytes: BigInt.from(64 * 1024 * 1024 + 1)),
        alter(digest: [1, 2]),
      ]) {
        expect(
          () => EditorDraftImportCodec.encodeRequest(value),
          throwsFormatException,
        );
      }
    },
  );

  test(
    'record envelope binds nested request and rejects missing Ready owner',
    () {
      final request = proposal();
      final valid = EditorDraftImportCodec.decodeEnvelope(envelope(request));
      expect(valid.kind, EditorDraftImportResultKind.record);
      expect(valid.record!.phase, EditorDraftImportPhase.ready);
      expect(valid.record!.bytesRetained, isTrue);
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          envelope(request, nestedOperation: 'foreign'),
        ),
        throwsFormatException,
      );
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          envelope(request, active: false),
        ),
        throwsFormatException,
      );
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          envelope(request, retained: false),
        ),
        throwsFormatException,
      );
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          envelope(request, badDigest: true),
        ),
        throwsFormatException,
      );
      final trailing = Uint8List.fromList([...envelope(request), 0]);
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(trailing),
        throwsFormatException,
      );
    },
  );
  test(
    'decision responses prove exact original operation and durable status',
    () {
      final request = proposal();
      final pending = EditorDraftImportCodec.decodeEnvelope(
        decisionEnvelope(request),
      );
      expect(pending.decision!.status, EditorDraftImportDecisionStatus.pending);
      expect(pending.decision!.intent.operation, 'abandon-original');
      final committed = EditorDraftImportCodec.decodeEnvelope(
        decisionEnvelope(
          request,
          status: wire.DecisionStatus.committed,
          committedRevision: 3,
        ),
      );
      expect(committed.decision!.committedRevision, BigInt.from(3));
      final historicalOnly = EditorDraftImportCodec.decodeEnvelope(
        decisionEnvelope(
          request,
          status: wire.DecisionStatus.committed,
          committedRevision: 3,
          decisionRevision: 0,
        ),
      );
      expect(
        historicalOnly.decision!.status,
        EditorDraftImportDecisionStatus.committed,
      );
      expect(historicalOnly.decision!.decisionRevision, BigInt.zero);
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          decisionEnvelope(request, decisionRevision: 0),
        ),
        throwsFormatException,
      );
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          decisionEnvelope(request, wrongOriginal: true),
        ),
        throwsFormatException,
      );
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(
          decisionEnvelope(request, status: wire.DecisionStatus.committed),
        ),
        throwsFormatException,
      );
    },
  );

  test('global scope pages reveal inactive drafts with exact page context', () {
    final page = EditorDraftImportCodec.decodeEnvelope(scopeEnvelope());
    expect(page.kind, EditorDraftImportResultKind.scopes);
    expect(page.requestCursor, 'cursor-one');
    expect(page.requestLimit, 2);
    expect(page.nextCursor, 'cursor-two');
    expect(page.scopes, hasLength(1));
    expect(page.scopes!.single.cardId, 'card-one');
    expect(page.scopes!.single.draftId, 'draft-one');
    expect(page.mainActive, isFalse);
    final empty = EditorDraftImportCodec.decodeEnvelope(
      scopeEnvelope(cursor: 'cursor-two', nextCursor: '', count: 0),
    );
    expect(empty.scopes, isEmpty);
    expect(empty.nextCursor, isEmpty);
    for (final bad in [
      scopeEnvelope(card: 'card-one'),
      scopeEnvelope(active: true),
      scopeEnvelope(limit: 0),
      scopeEnvelope(limit: 1, count: 2),
      scopeEnvelope(count: 2, duplicate: true),
      scopeEnvelope(cursor: List<String>.filled(513, 'x').join()),
    ]) {
      expect(
        () => EditorDraftImportCodec.decodeEnvelope(bad),
        throwsFormatException,
      );
    }
  });

  test('decision pages preserve exact cursor, limit, and empty-page shape', () {
    final request = proposal();
    final page = EditorDraftImportCodec.decodeEnvelope(
      decisionEnvelope(request, list: true),
    );
    expect(page.kind, EditorDraftImportResultKind.decisions);
    expect(page.requestCursor, 'cursor-one');
    expect(page.requestLimit, 2);
    expect(page.nextCursor, 'cursor-two');
    expect(page.decisions, hasLength(1));
    final empty = EditorDraftImportCodec.decodeEnvelope(
      decisionEnvelope(request, list: true, count: 0),
    );
    expect(empty.decisions, isEmpty);
    expect(empty.nextCursor, isEmpty);
    expect(
      () => EditorDraftImportCodec.decodeEnvelope(
        decisionEnvelope(request, list: true, limit: 0),
      ),
      throwsFormatException,
    );
  });
}
