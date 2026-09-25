import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/generated/editor_draft_api.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;

EditorDraftTextValue raw(
  String text, {
  int base = -1,
  int extent = -1,
  int composingStart = -1,
  int composingEnd = -1,
}) => EditorDraftTextValue(
  text: text,
  selectionBase: base,
  selectionExtent: extent,
  affinity: 1,
  directional: true,
  composingStart: composingStart,
  composingEnd: composingEnd,
);

EditorDraftWriteRequest request({
  String title = '',
  List<String> aliases = const ['a\u0000b'],
}) => EditorDraftWriteRequest(
  cardId: 'card-1',
  draftId: 'draft-1',
  operation: 'draft-op-1',
  expectedGeneration: BigInt.zero,
  sourceRevision: BigInt.one,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: EditorDraftValues(
    title: raw(title, base: 2, extent: 1, composingStart: 1, composingEnd: 3),
    description: raw(''),
    hypothesis: raw(''),
    conclusion: raw(''),
    todos: raw(''),
    category: '',
    stage: '',
  ),
  assets: [
    EditorDraftAssetSelection(
      origin: EditorDraftAssetOrigin.source,
      assetId: 'asset-1',
      aliases: aliases,
    ),
  ],
);

EditorDraftWriteRequest newCardRequest({
  EditorDraftAssetOrigin origin = EditorDraftAssetOrigin.staged,
  BigInt? sourceRevision,
  BigInt? expectedGeneration,
  String predecessorOperation = '',
  List<int> predecessorDigest = const [],
}) {
  final base = request(title: 'A😀B');
  return EditorDraftWriteRequest(
    cardId: base.cardId,
    draftId: base.draftId,
    operation: base.operation,
    expectedGeneration: expectedGeneration ?? BigInt.zero,
    sourceRevision: sourceRevision ?? BigInt.zero,
    sourceKind: EditorDraftSourceKind.newCard,
    predecessorOperation: predecessorOperation,
    predecessorDigest: predecessorDigest,
    values: base.values,
    assets: [
      EditorDraftAssetSelection(
        origin: origin,
        assetId: 'staged-1',
        aliases: const ['local.bin'],
      ),
    ],
  );
}

void writeText(wire.TextValueBuilder out, EditorDraftTextValue value) {
  out.text = value.text;
  out.selectionBase = value.selectionBase;
  out.selectionExtent = value.selectionExtent;
  out.affinity = value.affinity;
  out.directional = value.directional;
  out.composingStart = value.composingStart;
  out.composingEnd = value.composingEnd;
}

void writeSelection(
  wire.AssetSelectionBuilder out,
  EditorDraftAssetSelection value,
) {
  out.origin = value.origin.index;
  out.assetId = value.assetId;
  final aliases = out.initAliases(value.aliases.length);
  for (var i = 0; i < value.aliases.length; i++) {
    aliases[i] = value.aliases[i];
  }
}

void writeRequest(wire.WriteRequestBuilder out, EditorDraftWriteRequest value) {
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.cardId = value.cardId;
  out.draftId = value.draftId;
  out.operation = value.operation;
  out.expectedGenerationBigInt = value.expectedGeneration;
  out.sourceRevisionBigInt = value.sourceRevision;
  out.sourceKind = value.sourceKind.index;
  out.predecessorOperation = value.predecessorOperation;
  out.predecessorDigest = Uint8List.fromList(value.predecessorDigest);
  final fields = out.initValues();
  writeText(fields.initTitle(), value.values.title);
  writeText(fields.initDescription(), value.values.description);
  writeText(fields.initHypothesis(), value.values.hypothesis);
  writeText(fields.initConclusion(), value.values.conclusion);
  writeText(fields.initTodos(), value.values.todos);
  fields.category = value.values.category;
  fields.stage = value.values.stage;
  final assets = out.initAssets(value.assets.length);
  for (var i = 0; i < value.assets.length; i++) {
    writeSelection(assets[i], value.assets[i]);
  }
}

Uint8List frame(
  wire.ResultKind kind, {
  EditorDraftWriteRequest? draft,
  void Function(wire.EnvelopeBuilder, wire.RecordBuilder)? mutateRecord,
  bool badDigest = false,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(
    badDigest ? List<int>.filled(32, 0) : contract.editor_draft_apiDigest,
  );
  out.kind = kind;
  out.cardId = kind == wire.ResultKind.list ? '' : 'card-1';
  out.draftId = kind == wire.ResultKind.list ? '' : 'draft-1';
  out.operation = kind == wire.ResultKind.record ? 'draft-op-1' : '';
  out.expectedGeneration = 0;
  if (kind == wire.ResultKind.record) {
    final source = draft ?? request(title: 'A😀B');
    final record = out.initRecord();
    writeRequest(record.initRequest(), source);
    record.generation = 1;
    record.active = true;
    record.currentGeneration = 1;
    record.currentActive = true;
    record.sourceFormat = source.sourceKind == EditorDraftSourceKind.newCard
        ? 0
        : 2;
    record.sourceRevisionBigInt = source.sourceRevision;
    record.sourceSha256 = source.sourceKind == EditorDraftSourceKind.newCard
        ? Uint8List(0)
        : Uint8List.fromList(List<int>.filled(32, 7));
    record.predecessorRevision = 0;
    record.predecessorSha256 = Uint8List(0);
    record.requestSha256 = Uint8List.fromList(List<int>.filled(32, 9));
    final assets = record.initAssets(1);
    final asset = assets[0];
    writeSelection(asset.initSelection(), source.assets.single);
    asset.name = 'test.bin';
    asset.mediaType = 'application/octet-stream';
    asset.bytes = 3;
    asset.sha256 = Uint8List.fromList(List<int>.filled(32, 8));
    mutateRecord?.call(out, record);
  } else if (kind == wire.ResultKind.list) {
    out.initSummaries(0);
  } else if (kind == wire.ResultKind.imported) {
    final asset = out.initAsset();
    asset.id = 'asset-1';
    asset.name = 'test.bin';
    asset.kind = 'file';
    asset.bytes = -1; // Signed Dart carrier for u64::MAX.
  }
  return message.serialize();
}

void main() {
  test('write preserves empty raw fields, UTF-16 offsets and u64', () {
    final source = request(title: 'A😀B');
    final bytes = EditorDraftCodec.encodeWrite(source);
    final wireValue = MessageReader.deserialize(
      bytes,
    ).getRoot(wire.writeRequestFactory);
    expect(wireValue.values!.title!.text, 'A😀B');
    expect(wireValue.values!.title!.selectionBase, 2); // Inside surrogate pair.
    expect(wireValue.values!.description!.text, '');
    expect(wireValue.values!.title!.affinity, 1);
    expect(wireValue.values!.title!.directional, isTrue);
    expect(wireValue.values!.title!.composingStart, 1);
    expect(wireValue.assets!.single.aliases!.single, 'a\u0000b');
    expect(
      () => source.assets.add(source.assets.single),
      throwsUnsupportedError,
    );
    expect(
      () => source.assets.single.aliases[0] = 'changed',
      throwsUnsupportedError,
    );
    for (final number in [
      BigInt.one << 53,
      BigInt.one << 63,
      EditorDraftCodec.unsigned(-1),
    ]) {
      expect(
        EditorDraftCodec.unsigned(EditorDraftCodec.wireU64(number)),
        number,
      );
    }
  });

  test('new-card write binds kind zero source and staged evidence only', () {
    final fresh = newCardRequest();
    final encoded = EditorDraftCodec.encodeWrite(fresh);
    final decoded = MessageReader.deserialize(
      encoded,
    ).getRoot(wire.writeRequestFactory);
    expect(decoded.sourceKind, EditorDraftSourceKind.newCard.index);
    expect(decoded.sourceRevision, 0);
    expect(decoded.predecessorOperation, '');
    expect(decoded.predecessorDigest, isEmpty);
    expect(
      MessageReader.deserialize(
        EditorDraftCodec.encodeWrite(request(title: 'A😀B')),
      ).getRoot(wire.writeRequestFactory).sourceKind,
      EditorDraftSourceKind.existingCard.index,
    );
    for (final invalid in [
      newCardRequest(sourceRevision: BigInt.one),
      newCardRequest(
        predecessorOperation: 'old-op',
        predecessorDigest: List<int>.filled(32, 1),
      ),
      newCardRequest(origin: EditorDraftAssetOrigin.source),
      newCardRequest(origin: EditorDraftAssetOrigin.predecessor),
      newCardRequest(origin: EditorDraftAssetOrigin.previousDraft),
    ]) {
      expect(
        () => EditorDraftCodec.encodeWrite(invalid),
        throwsFormatException,
      );
    }
    expect(
      () => EditorDraftCodec.encodeWrite(
        newCardRequest(
          origin: EditorDraftAssetOrigin.previousDraft,
          expectedGeneration: BigInt.one,
        ),
      ),
      returnsNormally,
    );
  });

  test('new-card record has no invented source and rejects unknown kinds', () {
    final fresh = newCardRequest();
    final result = EditorDraftCodec.decodeEnvelope(
      frame(wire.ResultKind.record, draft: fresh),
    );
    expect(result.record!.request.sourceKind, EditorDraftSourceKind.newCard);
    expect(result.record!.sourceFormat, 0);
    expect(result.record!.sourceRevision, BigInt.zero);
    expect(result.record!.sourceSha256, isEmpty);
    for (final mutation
        in <void Function(wire.EnvelopeBuilder, wire.RecordBuilder)>[
          (_, record) => record.sourceFormat = 2,
          (_, record) =>
              record.sourceSha256 = Uint8List.fromList(List<int>.filled(32, 7)),
          (_, record) {
            final nested = record.initRequest();
            writeRequest(nested, fresh);
            nested.sourceKind = 2;
          },
        ]) {
      expect(
        () => EditorDraftCodec.decodeEnvelope(
          frame(wire.ResultKind.record, draft: fresh, mutateRecord: mutation),
        ),
        throwsFormatException,
      );
    }
  });
  test('write rejects lossy surrogates, invalid bounds and identity', () {
    expect(
      () => EditorDraftCodec.encodeWrite(
        request(title: String.fromCharCode(0xd800)),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.encodeWrite(request(title: 'A')),
      throwsFormatException,
    ); // Selection/composition exceed one code unit.
    final good = request(title: 'A😀B');
    expect(
      () => EditorDraftCodec.encodeWrite(
        EditorDraftWriteRequest(
          cardId: 'bad/id',
          draftId: good.draftId,
          operation: good.operation,
          expectedGeneration: good.expectedGeneration,
          sourceRevision: good.sourceRevision,
          predecessorOperation: '',
          predecessorDigest: const [],
          values: good.values,
          assets: good.assets,
        ),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.encodeWrite(
        EditorDraftWriteRequest(
          cardId: good.cardId,
          draftId: good.draftId,
          operation: good.operation,
          expectedGeneration: -BigInt.one,
          sourceRevision: good.sourceRevision,
          predecessorOperation: '',
          predecessorDigest: const [],
          values: good.values,
          assets: good.assets,
        ),
      ),
      throwsFormatException,
    );
  });

  test('record binds request, assets, revisions and immutable hashes', () {
    final result = EditorDraftCodec.decodeEnvelope(
      frame(wire.ResultKind.record),
    );
    expect(result.kind, EditorDraftResultKind.record);
    expect(result.cardId, 'card-1');
    expect(result.record!.request.values.title.text, 'A😀B');
    expect(result.record!.generation, BigInt.one);
    expect(result.record!.assets.single.sha256, List<int>.filled(32, 8));
    expect(() => result.record!.sourceSha256[0] = 0, throwsUnsupportedError);
    expect(
      () => result.record!.assets.add(result.record!.assets.single),
      throwsUnsupportedError,
    );
  });

  test(
    'historical inactive record retains original request but reports now',
    () {
      final result = EditorDraftCodec.decodeEnvelope(
        frame(
          wire.ResultKind.record,
          mutateRecord: (out, record) {
            record.generation = 2;
            record.active = false;
            record.currentGeneration = 2;
            record.currentActive = false;
            record.repeated = true;
          },
        ),
      );
      expect(result.record!.request.expectedGeneration, BigInt.zero);
      expect(result.record!.generation, BigInt.from(2));
      expect(result.record!.active, isFalse);
    },
  );

  test('rejects changed identity, selection alias, hashes and protocol', () {
    for (final bytes in [
      frame(wire.ResultKind.record, badDigest: true),
      frame(
        wire.ResultKind.record,
        mutateRecord: (out, _) {
          out.cardId = 'another-card';
        },
      ),
      frame(
        wire.ResultKind.record,
        mutateRecord: (_, record) {
          record.sourceSha256 = Uint8List(31);
        },
      ),
      frame(
        wire.ResultKind.record,
        mutateRecord: (_, record) {
          final changed = record.initAssets(1)[0];
          final selection = changed.initSelection();
          selection.origin = 0;
          selection.assetId = 'asset-1';
          final aliases = selection.initAliases(2);
          aliases[0] = 'a';
          aliases[1] = 'b';
          changed.name = 'test.bin';
          changed.mediaType = 'application/octet-stream';
          changed.bytes = 3;
          changed.sha256 = Uint8List.fromList(List<int>.filled(32, 8));
        },
      ),
    ]) {
      expect(
        () => EditorDraftCodec.decodeEnvelope(bytes),
        throwsFormatException,
      );
    }
  });

  test('kind shapes and exact frame boundaries are enforced', () {
    expect(
      EditorDraftCodec.decodeEnvelope(frame(wire.ResultKind.absent)).record,
      isNull,
    );
    expect(
      EditorDraftCodec.decodeEnvelope(frame(wire.ResultKind.list)).summaries,
      isEmpty,
    );
    final imported = EditorDraftCodec.decodeEnvelope(
      frame(wire.ResultKind.imported),
    );
    expect(imported.asset!.bytes, EditorDraftCodec.unsigned(-1));
    expect(
      EditorDraftCodec.decodeEnvelope(frame(wire.ResultKind.exported)).kind,
      EditorDraftResultKind.exported,
    );
    final valid = frame(wire.ResultKind.record);
    expect(
      () => EditorDraftCodec.decodeEnvelope(Uint8List.fromList([...valid, 0])),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        Uint8List.sublistView(valid, 0, valid.length - 1),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        Uint8List(EditorDraftCodec.maxFrameBytes + 1),
      ),
      throwsFormatException,
    );
  });
  test('large valid record decodes bounded duplicate aliases', () {
    const kib = 1024;
    final chunk = 'x' * (512 * kib);
    final base = request(title: 'A😀B');
    final large = EditorDraftWriteRequest(
      cardId: base.cardId,
      draftId: base.draftId,
      operation: base.operation,
      expectedGeneration: BigInt.zero,
      sourceRevision: BigInt.one,
      predecessorOperation: '',
      predecessorDigest: const [],
      values: EditorDraftValues(
        title: base.values.title,
        description: raw(chunk),
        hypothesis: raw(chunk),
        conclusion: raw(''),
        todos: raw(''),
        category: '',
        stage: '',
      ),
      assets: [
        for (var i = 0; i < 20; i++)
          EditorDraftAssetSelection(
            origin: EditorDraftAssetOrigin.source,
            assetId: 'asset-$i',
            aliases: [
              for (var j = 0; j < 8; j++)
                ('a${i}_${j}_').padRight(16 * kib, 'x'),
            ],
          ),
      ],
    );
    expect(
      EditorDraftCodec.encodeWrite(large).length,
      lessThan(EditorDraftCodec.maxFrameBytes),
    );
    final message = MessageBuilder();
    final out = message.initRoot(wire.envelopeFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
    out.kind = wire.ResultKind.record;
    out.cardId = large.cardId;
    out.draftId = large.draftId;
    out.operation = large.operation;
    out.expectedGeneration = 0;
    final record = out.initRecord();
    writeRequest(record.initRequest(), large);
    record.generation = 1;
    record.active = true;
    record.currentGeneration = 1;
    record.currentActive = true;
    record.sourceFormat = 2;
    record.sourceRevision = 1;
    record.sourceSha256 = Uint8List.fromList(List<int>.filled(32, 7));
    record.predecessorRevision = 0;
    record.predecessorSha256 = Uint8List(0);
    record.requestSha256 = Uint8List.fromList(List<int>.filled(32, 9));
    final stored = record.initAssets(large.assets.length);
    for (var i = 0; i < large.assets.length; i++) {
      final item = stored[i];
      writeSelection(item.initSelection(), large.assets[i]);
      item.name = 'file.bin';
      item.mediaType = 'application/octet-stream';
      item.bytes = 1;
      item.sha256 = Uint8List.fromList(List<int>.filled(32, i));
    }
    final bytes = message.serialize();
    expect(bytes.length, greaterThan(5 * 1024 * 1024));
    expect(bytes.length, lessThan(EditorDraftCodec.maxFrameBytes));
    final decoded = EditorDraftCodec.decodeEnvelope(bytes);
    expect(decoded.record!.request.values.description.text, chunk);
    expect(decoded.record!.assets.length, 20);
    expect(decoded.record!.assets.last.selection.aliases.length, 8);
  });
}
