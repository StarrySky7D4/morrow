import 'dart:convert';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';

import 'editor_draft_import.dart';
import 'generated/editor_draft_staging_api.capnp.dart' as wire;
import 'generated/identity.dart' as contract;
import 'versioned_content_codec.dart';

abstract final class EditorDraftImportCodec {
  static const int maxFrameBytes = 8 * 1024 * 1024;
  static const int maxRequestBytes = 64 * 1024;
  static const int maxImportBytes = 64 * 1024 * 1024;
  static final BigInt _maxU64 = VersionedContentCodec.maxU64;

  static BigInt unsigned(int carrier) =>
      VersionedContentCodec.unsigned(carrier);
  static int wireU64(BigInt value) => VersionedContentCodec.wireU64(value);

  static bool sameBytes(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static bool sameRequest(
    EditorDraftImportRequest a,
    EditorDraftImportRequest b,
  ) =>
      a.cardId == b.cardId &&
      a.draftId == b.draftId &&
      a.operation == b.operation &&
      a.expectedGeneration == b.expectedGeneration &&
      a.name == b.name &&
      a.kind == b.kind &&
      a.bytes == b.bytes &&
      sameBytes(a.sha256, b.sha256);

  static void _string(String value, int maxBytes) {
    final units = value.codeUnits;
    for (var i = 0; i < units.length; i++) {
      final unit = units[i];
      if (unit >= 0xd800 && unit <= 0xdbff) {
        if (++i >= units.length || units[i] < 0xdc00 || units[i] > 0xdfff) {
          throw const FormatException('Unpaired draft import surrogate');
        }
      } else if (unit >= 0xdc00 && unit <= 0xdfff) {
        throw const FormatException('Unpaired draft import surrogate');
      }
    }
    if (utf8.encode(value).length > maxBytes) {
      throw const FormatException('Draft import text limit');
    }
  }

  static void identity(String value) {
    _string(value, 256);
    if (value.isEmpty ||
        value.runes.any(
          (rune) =>
              rune < 0x20 ||
              (rune >= 0x7f && rune <= 0x9f) ||
              rune == 0x2f ||
              rune == 0x5c ||
              rune == 0x3a,
        )) {
      throw const FormatException('Invalid draft import identity');
    }
  }

  static void validateCursor(String value) => _string(value, 512);

  static void operation(String value) {
    identity(value);
    if (value.startsWith('draft-import-stage-')) {
      throw const FormatException('Reserved draft import operation');
    }
  }

  static void u64(BigInt value, {bool positive = false, bool canBeMax = true}) {
    if (value < (positive ? BigInt.one : BigInt.zero) ||
        value > _maxU64 ||
        (!canBeMax && value == _maxU64)) {
      throw const FormatException('Draft import unsigned value out of range');
    }
  }

  static void _hash(List<int> bytes) {
    if (bytes.length != 32 || bytes.any((byte) => byte < 0 || byte > 255)) {
      throw const FormatException('Invalid draft import SHA-256');
    }
  }

  static void validateRequest(EditorDraftImportRequest request) {
    identity(request.cardId);
    identity(request.draftId);
    operation(request.operation);
    u64(request.expectedGeneration, positive: true, canBeMax: false);
    _string(request.name, 16 * 1024);
    _string(request.kind, 1024);
    if (request.name.isEmpty || request.kind.isEmpty) {
      throw const FormatException('Empty draft import metadata');
    }
    u64(request.bytes);
    if (request.bytes > BigInt.from(maxImportBytes)) {
      throw const FormatException('Draft import byte limit');
    }
    _hash(request.sha256);
  }

  static void validateAbandon(EditorDraftImportAbandon intent) {
    identity(intent.cardId);
    identity(intent.draftId);
    operation(intent.importOperation);
    operation(intent.operation);
    u64(intent.currentGeneration, positive: true, canBeMax: false);
    if (intent.operation.startsWith('draft-import-decision-prepare-') ||
        intent.operation == intent.importOperation) {
      throw const FormatException('Draft import cannot abandon itself');
    }
  }

  static Uint8List encodeRequest(EditorDraftImportRequest request) {
    validateRequest(request);
    final message = MessageBuilder();
    final out = message.initRoot(wire.importRequestFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.editor_draft_staging_apiDigest);
    out.cardId = request.cardId;
    out.draftId = request.draftId;
    out.operation = request.operation;
    out.expectedGeneration = wireU64(request.expectedGeneration);
    out.name = request.name;
    out.kind = request.kind;
    out.bytes = wireU64(request.bytes);
    out.sha256 = Uint8List.fromList(request.sha256);
    final bytes = message.serialize();
    if (bytes.length > maxRequestBytes) {
      throw const FormatException('Draft import request frame limit');
    }
    return bytes;
  }

  static MessageReader _read(Uint8List bytes) {
    if (bytes.length < 8 || bytes.length > maxFrameBytes) {
      throw const FormatException('Draft import frame length');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('Draft import segment count');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) {
      throw const FormatException('Draft import frame header');
    }
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) {
        throw const FormatException('Draft import segment length');
      }
    }
    if (total != bytes.length) {
      throw const FormatException('Trailing draft import frame data');
    }
    return MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: maxFrameBytes ~/ 8 * 4,
        nestingLimit: 24,
        maxSegments: 512,
      ),
    );
  }

  static void _header(int version, Uint8List? digest) {
    if (version != 1 ||
        digest == null ||
        !sameBytes(digest, contract.editor_draft_staging_apiDigest)) {
      throw const FormatException('Draft import protocol mismatch');
    }
  }

  static EditorDraftImportRequest _decodeRequest(
    wire.ImportRequestReader? value,
  ) {
    if (value == null) {
      throw const FormatException('Missing draft import request');
    }
    _header(value.version, value.digest);
    if (value.cardId == null ||
        value.draftId == null ||
        value.operation == null ||
        value.name == null ||
        value.kind == null ||
        value.sha256 == null) {
      throw const FormatException('Missing draft import request field');
    }
    final request = EditorDraftImportRequest(
      cardId: value.cardId!,
      draftId: value.draftId!,
      operation: value.operation!,
      expectedGeneration: unsigned(value.expectedGeneration),
      name: value.name!,
      kind: value.kind!,
      bytes: unsigned(value.bytes),
      sha256: value.sha256!,
    );
    validateRequest(request);
    return request;
  }

  static EditorDraftImportRecord _decodeRecord(wire.RecordReader? value) {
    if (value == null || value.assetId == null || value.phase == null) {
      throw const FormatException('Missing draft import record');
    }
    final request = _decodeRequest(value.request);
    identity(value.assetId!);
    final revision = unsigned(value.stagingRevision);
    final currentGeneration = unsigned(value.currentGeneration);
    if (revision == BigInt.zero ||
        revision > BigInt.from(256) ||
        currentGeneration < request.expectedGeneration ||
        (value.currentActive &&
            (!value.mainActive || value.phase != wire.Phase.ready)) ||
        (value.phase == wire.Phase.ready &&
            (!value.currentActive ||
                !value.mainActive ||
                !value.bytesRetained)) ||
        (value.phase == wire.Phase.pending && value.currentActive) ||
        (value.phase == wire.Phase.retired && value.currentActive)) {
      throw const FormatException('Invalid draft import record state');
    }
    return EditorDraftImportRecord(
      request: request,
      assetId: value.assetId!,
      phase: EditorDraftImportPhase.values[value.phase!.index],
      currentActive: value.currentActive,
      bytesRetained: value.bytesRetained,
      stagingRevision: revision,
      repeated: value.repeated,
      currentGeneration: currentGeneration,
      mainActive: value.mainActive,
    );
  }

  static EditorDraftImportDecision _decodeDecision(wire.DecisionReader? value) {
    if (value == null || value.operation == null || value.status == null) {
      throw const FormatException('Missing draft import decision');
    }
    final request = _decodeRequest(value.request);
    final expected = unsigned(value.expectedGeneration);
    final current = unsigned(value.currentGeneration);
    final staging = unsigned(value.stagingRevision);
    final decisionRevision = unsigned(value.decisionRevision);
    final committedRevision = unsigned(value.committedRevision);
    final intent = EditorDraftImportAbandon(
      cardId: request.cardId,
      draftId: request.draftId,
      importOperation: request.operation,
      operation: value.operation!,
      currentGeneration: expected,
    );
    validateAbandon(intent);
    if (expected < request.expectedGeneration ||
        (value.status == wire.DecisionStatus.cancelled
            ? decisionRevision != BigInt.two
            : value.status == wire.DecisionStatus.committed
            ? decisionRevision != BigInt.zero && decisionRevision != BigInt.one
            : decisionRevision != BigInt.one) ||
        (value.status == wire.DecisionStatus.committed
            ? committedRevision == BigInt.zero
            : committedRevision != BigInt.zero) ||
        staging > BigInt.from(256)) {
      throw const FormatException('Invalid draft import decision state');
    }
    return EditorDraftImportDecision(
      request: request,
      operation: value.operation!,
      expectedGeneration: expected,
      status: EditorDraftImportDecisionStatus.values[value.status!.index],
      currentGeneration: current,
      mainActive: value.mainActive,
      stagingRevision: staging,
      decisionRevision: decisionRevision,
      committedRevision: committedRevision,
    );
  }

  static EditorDraftImportSnapshot decodeEnvelope(Uint8List bytes) {
    try {
      final value = _read(bytes).getRoot(wire.envelopeFactory);
      _header(value.version, value.digest);
      if (value.kind == null ||
          value.cardId == null ||
          value.draftId == null ||
          value.operation == null ||
          value.importOperation == null) {
        throw const FormatException('Missing draft import envelope context');
      }
      final kind = EditorDraftImportResultKind.values[value.kind!.index];
      final card = value.cardId!;
      final draft = value.draftId!;
      final op = value.operation!;
      final importOp = value.importOperation!;
      final expected = unsigned(value.expectedGeneration);
      final current = unsigned(value.currentGeneration);
      final staging = unsigned(value.stagingRevision);
      u64(expected);
      u64(current);
      u64(staging);
      if (card.isNotEmpty) identity(card);
      if (draft.isNotEmpty) identity(draft);
      if (op.isNotEmpty) operation(op);
      if (importOp.isNotEmpty) operation(importOp);
      EditorDraftImportRecord? record;
      List<EditorDraftImportRecord>? records;
      List<int>? exportedHash;
      EditorDraftImportDecision? decision;
      List<EditorDraftImportDecision>? decisions;
      List<EditorDraftImportScope>? scopes;
      final requestCursor = value.requestCursor ?? '';
      final nextCursor = value.nextCursor ?? '';
      final requestLimit = value.requestLimit;
      final exportedBytes = unsigned(value.exportBytes);
      if (kind != EditorDraftImportResultKind.decision &&
          kind != EditorDraftImportResultKind.decisions &&
          kind != EditorDraftImportResultKind.scopes &&
          (value.decision != null ||
              value.decisions != null ||
              requestCursor.isNotEmpty ||
              nextCursor.isNotEmpty ||
              requestLimit != 0)) {
        throw const FormatException('Unexpected draft import decision fields');
      }
      if (kind != EditorDraftImportResultKind.scopes && value.scopes != null) {
        throw const FormatException('Unexpected global scope list');
      }
      switch (kind) {
        case EditorDraftImportResultKind.absent:
          if (value.record != null ||
              value.records != null ||
              value.exportSha256?.isNotEmpty == true ||
              exportedBytes != BigInt.zero) {
            throw const FormatException('Invalid absent draft import');
          }
        case EditorDraftImportResultKind.record:
          if (value.records != null ||
              value.exportSha256?.isNotEmpty == true ||
              exportedBytes != BigInt.zero) {
            throw const FormatException('Invalid draft import record envelope');
          }
          record = _decodeRecord(value.record);
          if (record.request.cardId != card ||
              record.request.draftId != draft ||
              record.request.operation != importOp ||
              record.currentGeneration != current ||
              record.mainActive != value.mainActive ||
              record.stagingRevision != staging) {
            throw const FormatException(
              'Draft import nested identity mismatch',
            );
          }
        case EditorDraftImportResultKind.list:
        case EditorDraftImportResultKind.reconciled:
          if (value.record != null ||
              value.records == null ||
              value.exportSha256?.isNotEmpty == true ||
              exportedBytes != BigInt.zero ||
              value.records!.length > 20) {
            throw const FormatException('Invalid draft import list');
          }
          records = [for (final item in value.records!) _decodeRecord(item)];
          final seen = <String>{};
          for (final item in records) {
            if (item.request.cardId != card ||
                item.request.draftId != draft ||
                item.currentGeneration != current ||
                item.mainActive != value.mainActive ||
                item.stagingRevision != staging ||
                !seen.add(item.request.operation)) {
              throw const FormatException(
                'Draft import list identity mismatch',
              );
            }
          }
        case EditorDraftImportResultKind.exported:
          if (value.record != null ||
              value.records != null ||
              value.exportSha256 == null) {
            throw const FormatException('Invalid draft import export envelope');
          }
          _hash(value.exportSha256!);
          exportedHash = value.exportSha256!;
          if (exportedBytes > BigInt.from(maxImportBytes)) {
            throw const FormatException('Draft import export size limit');
          }
        case EditorDraftImportResultKind.decision:
          if (value.record != null ||
              value.records != null ||
              value.decisions != null ||
              value.exportSha256?.isNotEmpty == true ||
              exportedBytes != BigInt.zero ||
              requestCursor.isNotEmpty ||
              nextCursor.isNotEmpty ||
              requestLimit != 0) {
            throw const FormatException('Invalid decision envelope');
          }
          decision = _decodeDecision(value.decision);
          if (decision.request.cardId != card ||
              decision.request.draftId != draft ||
              decision.request.operation != importOp ||
              decision.operation != op ||
              decision.expectedGeneration != expected ||
              decision.currentGeneration != current ||
              decision.mainActive != value.mainActive ||
              decision.stagingRevision != staging) {
            throw const FormatException('Decision envelope identity mismatch');
          }
        case EditorDraftImportResultKind.decisions:
          if (value.record != null ||
              value.records != null ||
              value.decision != null ||
              value.decisions == null ||
              value.decisions!.length > 32 ||
              value.exportSha256?.isNotEmpty == true ||
              exportedBytes != BigInt.zero ||
              requestLimit < 1 ||
              requestLimit > 32) {
            throw const FormatException('Invalid decision page envelope');
          }
          _string(requestCursor, 512);
          _string(nextCursor, 512);
          decisions = [
            for (final item in value.decisions!) _decodeDecision(item),
          ];
          final seen = <String>{};
          for (final item in decisions) {
            if (item.request.cardId != card ||
                item.request.draftId != draft ||
                item.currentGeneration != current ||
                item.mainActive != value.mainActive ||
                item.stagingRevision != staging ||
                !seen.add(item.operation)) {
              throw const FormatException('Decision page identity mismatch');
            }
          }
        case EditorDraftImportResultKind.scopes:
          if (card.isNotEmpty ||
              draft.isNotEmpty ||
              op.isNotEmpty ||
              importOp.isNotEmpty ||
              expected != BigInt.zero ||
              current != BigInt.zero ||
              staging != BigInt.zero ||
              value.mainActive ||
              value.record != null ||
              value.records != null ||
              value.decision != null ||
              value.decisions != null ||
              value.scopes == null ||
              value.exportSha256?.isNotEmpty == true ||
              exportedBytes != BigInt.zero ||
              requestLimit < 1 ||
              requestLimit > 32 ||
              value.scopes!.length > requestLimit) {
            throw const FormatException('Invalid global decision scope page');
          }
          validateCursor(requestCursor);
          validateCursor(nextCursor);
          scopes = <EditorDraftImportScope>[];
          final seenScopes = <String>{};
          for (final row in value.scopes!) {
            if (row.cardId == null || row.draftId == null) {
              throw const FormatException('Missing decision scope identity');
            }
            identity(row.cardId!);
            identity(row.draftId!);
            if (!seenScopes.add(
              '${row.cardId!.length}:${row.cardId!}${row.draftId!}',
            )) {
              throw const FormatException('Duplicate decision scope');
            }
            scopes.add(
              EditorDraftImportScope(
                cardId: row.cardId!,
                draftId: row.draftId!,
              ),
            );
          }
      }
      return EditorDraftImportSnapshot(
        kind: kind,
        cardId: card,
        draftId: draft,
        operation: op,
        expectedGeneration: expected,
        importOperation: importOp,
        currentGeneration: current,
        mainActive: value.mainActive,
        stagingRevision: staging,
        record: record,
        records: records,
        exportSha256: exportedHash,
        exportBytes: exportedBytes,
        decision: decision,
        decisions: decisions,
        scopes: scopes,
        requestCursor: requestCursor,
        requestLimit: requestLimit,
        nextCursor: nextCursor,
      );
    } on FormatException {
      rethrow;
    } catch (_) {
      throw const FormatException('Malformed draft import envelope');
    }
  }
}
