import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/generated/editor_draft_api.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;

final _maxU64 = (BigInt.one << 64) - BigInt.one;
final _firstCursor = "morrow-host-editor-draft-${'1' * 64}";
final _secondCursor = "morrow-host-editor-draft-${'2' * 64}";

EditorDraftTextValue _text(String text) => EditorDraftTextValue(
  text: text,
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftWriteRequest _request({
  String draft = 'child-draft',
  String operation = 'child-op',
  BigInt? expected,
  BigInt? source,
  List<EditorDraftAssetSelection>? assets,
}) => EditorDraftWriteRequest(
  cardId: 'card',
  draftId: draft,
  operation: operation,
  expectedGeneration: expected ?? BigInt.zero,
  sourceRevision: source ?? BigInt.from(9),
  predecessorOperation: '',
  predecessorDigest: const [],
  values: EditorDraftValues(
    title: _text('raw😀'),
    description: _text('description'),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: 'category',
    stage: 'stage',
  ),
  assets:
      assets ??
      [
        EditorDraftAssetSelection(
          origin: EditorDraftAssetOrigin.parentDraft,
          assetId: 'parent-only-asset',
          aliases: const ['asset.bin'],
        ),
      ],
);

EditorDraftParentLink _link({
  BigInt? generation,
  List<int>? parentHash,
  List<int>? committedHash,
}) => EditorDraftParentLink(
  parentDraftId: 'parent-draft',
  parentGeneration: generation ?? BigInt.one,
  parentSaveOperation: 'parent-op',
  parentRequestSha256: parentHash ?? List<int>.filled(32, 7),
  committedOperation: 'captured-s1',
  committedSha256: committedHash ?? List<int>.filled(32, 8),
  childOperation: 'child-op',
);

void _writeText(wire.TextValueBuilder out, EditorDraftTextValue value) {
  out.text = value.text;
  out.selectionBase = value.selectionBase;
  out.selectionExtent = value.selectionExtent;
  out.affinity = value.affinity;
  out.directional = value.directional;
  out.composingStart = value.composingStart;
  out.composingEnd = value.composingEnd;
}

void _writeRequest(
  wire.WriteRequestBuilder out,
  EditorDraftWriteRequest request,
) {
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.cardId = request.cardId;
  out.draftId = request.draftId;
  out.operation = request.operation;
  out.expectedGeneration = EditorDraftCodec.wireU64(request.expectedGeneration);
  out.sourceRevision = EditorDraftCodec.wireU64(request.sourceRevision);
  out.sourceKind = request.sourceKind.index;
  out.predecessorOperation = request.predecessorOperation;
  out.predecessorDigest = Uint8List.fromList(request.predecessorDigest);
  final values = out.initValues();
  _writeText(values.initTitle(), request.values.title);
  _writeText(values.initDescription(), request.values.description);
  _writeText(values.initHypothesis(), request.values.hypothesis);
  _writeText(values.initConclusion(), request.values.conclusion);
  _writeText(values.initTodos(), request.values.todos);
  values.category = request.values.category;
  values.stage = request.values.stage;
  final assets = out.initAssets(request.assets.length);
  for (var i = 0; i < request.assets.length; i++) {
    assets[i].origin = request.assets[i].origin.index;
    assets[i].assetId = request.assets[i].assetId;
    final aliases = assets[i].initAliases(request.assets[i].aliases.length);
    for (var j = 0; j < request.assets[i].aliases.length; j++) {
      aliases[j] = request.assets[i].aliases[j];
    }
  }
}

void _writeLink(wire.ParentLinkBuilder out, EditorDraftParentLink link) {
  out.parentDraftId = link.parentDraftId;
  out.parentGeneration = EditorDraftCodec.wireU64(link.parentGeneration);
  out.parentSaveOperation = link.parentSaveOperation;
  out.parentRequestSha256 = Uint8List.fromList(link.parentRequestSha256);
  out.committedOperation = link.committedOperation;
  out.committedSha256 = Uint8List.fromList(link.committedSha256);
  out.childOperation = link.childOperation;
}

Uint8List _recordFrame({
  EditorDraftWriteRequest? request,
  bool active = true,
  bool includeRequestHash = true,
  void Function(wire.RecordBuilder)? mutate,
}) {
  final source = request ?? _request();
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.record;
  out.cardId = source.cardId;
  out.draftId = source.draftId;
  out.operation = source.operation;
  out.expectedGeneration = EditorDraftCodec.wireU64(source.expectedGeneration);
  final record = out.initRecord();
  _writeRequest(record.initRequest(), source);
  record.generation = active ? 1 : 2;
  record.active = active;
  record.currentGeneration = active ? 1 : 2;
  record.currentActive = active;
  record.repeated = false;
  record.sourceFormat = 2;
  record.sourceRevision = EditorDraftCodec.wireU64(source.sourceRevision);
  record.sourceSha256 = Uint8List(32);
  record.predecessorRevision = 0;
  record.predecessorSha256 = Uint8List(0);
  if (includeRequestHash) {
    record.requestSha256 = Uint8List.fromList(List<int>.filled(32, 7));
  }
  final assets = record.initAssets(source.assets.length);
  for (var i = 0; i < source.assets.length; i++) {
    final selected = source.assets[i];
    final stored = assets[i];
    final selection = stored.initSelection();
    selection.origin = selected.origin.index;
    selection.assetId = selected.assetId;
    final aliases = selection.initAliases(selected.aliases.length);
    for (var j = 0; j < selected.aliases.length; j++) {
      aliases[j] = selected.aliases[j];
    }
    stored.name = 'asset.bin';
    stored.mediaType = 'application/octet-stream';
    stored.bytes = 3;
    stored.sha256 = Uint8List(32);
  }
  mutate?.call(record);
  return message.serialize();
}

Uint8List _lineageFrame({
  String requestCursor = '',
  String? nextCursor,
  bool reverse = false,
  bool badContext = false,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.lineages;
  out.cardId = badContext ? 'smuggled-card' : '';
  out.draftId = '';
  out.operation = '';
  out.expectedGeneration = 0;
  out.requestCursor = requestCursor;
  out.requestLimit = 2;
  out.nextCursor = nextCursor ?? _secondCursor;
  final entries = out.initLineages(2);
  for (var i = 0; i < 2; i++) {
    final entry = entries[i];
    entry.cardId = 'card';
    entry.childDraftId = i == 0 ? 'child-one' : 'child-two';
    entry.childGeneration = EditorDraftCodec.wireU64(
      _maxU64 - BigInt.from(i + 1),
    );
    entry.childActive = true;
    entry.parentGeneration = 2;
    entry.parentActive = false;
    final link = _link();
    _writeLink(entry.initParentLink(), link);
    entry.cursor = (reverse ? i == 0 : i == 1) ? _secondCursor : _firstCursor;
  }
  return message.serialize();
}

void main() {
  test('handoff roundtrips exact high u64 and immutable evidence', () {
    final parentHash = List<int>.filled(32, 3);
    final committedHash = List<int>.filled(32, 4);
    final link = _link(
      generation: _maxU64 - BigInt.one,
      parentHash: parentHash,
      committedHash: committedHash,
    );
    parentHash[0] = 9;
    committedHash[0] = 9;
    expect(link.parentRequestSha256.first, 3);
    expect(link.committedSha256.first, 4);
    expect(() => link.committedSha256[0] = 1, throwsUnsupportedError);
    final handoff = EditorDraftHandoffRequest(
      request: _request(source: _maxU64),
      parentLink: link,
    );
    final bytes = EditorDraftCodec.encodeHandoff(handoff);
    final decoded = EditorDraftCodec.decodeHandoff(bytes);
    expect(decoded.request.sourceRevision, _maxU64);
    expect(
      decoded.request.assets.single.origin,
      EditorDraftAssetOrigin.parentDraft,
    );
    expect(decoded.parentLink.parentGeneration, _maxU64 - BigInt.one);
    expect(decoded.parentLink.parentRequestSha256.first, 3);
    expect(decoded.parentLink.committedSha256.first, 4);
    final ordinary = EditorDraftCodec.encodeWrite(handoff.request);
    expect(ordinary, isNotEmpty); // Shape is valid; host alone gates origin 4.
  });

  test('handoff refuses changed identity, digest and missing pointers', () {
    final base = _request();
    final wrongOperation = EditorDraftParentLink(
      parentDraftId: 'parent-draft',
      parentGeneration: BigInt.one,
      parentSaveOperation: 'parent-op',
      parentRequestSha256: List<int>.filled(32, 7),
      committedOperation: 'captured-s1',
      committedSha256: List<int>.filled(32, 8),
      childOperation: 'different-child-op',
    );
    expect(
      () => EditorDraftCodec.encodeHandoff(
        EditorDraftHandoffRequest(request: base, parentLink: wrongOperation),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.encodeHandoff(
        EditorDraftHandoffRequest(
          request: base,
          parentLink: _link(parentHash: List<int>.filled(31, 1)),
        ),
      ),
      throwsFormatException,
    );
    final message = MessageBuilder();
    final out = message.initRoot(wire.handoffRequestFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
    _writeRequest(out.initRequest(), base);
    expect(
      () => EditorDraftCodec.decodeHandoff(message.serialize()),
      throwsFormatException,
    );
    final newCard = _request();
    expect(
      () => EditorDraftCodec.encodeHandoff(
        EditorDraftHandoffRequest(
          request: EditorDraftWriteRequest(
            cardId: newCard.cardId,
            draftId: newCard.draftId,
            operation: newCard.operation,
            expectedGeneration: BigInt.zero,
            sourceRevision: BigInt.zero,
            sourceKind: EditorDraftSourceKind.newCard,
            predecessorOperation: '',
            predecessorDigest: const [],
            values: newCard.values,
            assets: const [],
          ),
          parentLink: _link(),
        ),
      ),
      throwsFormatException,
    );
  });

  test('record requires exact host request hash and binds parent origin', () {
    final linked = EditorDraftCodec.decodeEnvelope(
      _recordFrame(
        mutate: (record) => _writeLink(record.initParentLink(), _link()),
      ),
    );
    expect(linked.record!.requestSha256, List<int>.filled(32, 7));
    expect(linked.record!.parentLink!.parentDraftId, 'parent-draft');
    expect(() => linked.record!.requestSha256[0] = 9, throwsUnsupportedError);
    expect(
      () => EditorDraftCodec.decodeEnvelope(_recordFrame()),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _recordFrame(
          includeRequestHash: false,
          mutate: (record) => _writeLink(record.initParentLink(), _link()),
        ),
      ),
      throwsFormatException,
    );
    final parent = _request(
      draft: 'parent-draft',
      operation: 'parent-op',
      assets: const [],
    );
    final retired = EditorDraftCodec.decodeEnvelope(
      _recordFrame(
        request: parent,
        active: false,
        mutate: (record) {
          final marker = record.initRetirement();
          marker.childDraftId = 'child-draft';
          marker.childOperation = 'child-op';
          marker.operation = 'retire-op';
          marker.parentGeneration = 1;
        },
      ),
    );
    expect(retired.record!.retirement!.operation, 'retire-op');
    final middle = EditorDraftCodec.decodeEnvelope(
      _recordFrame(
        active: false,
        mutate: (record) {
          _writeLink(record.initParentLink(), _link());
          final marker = record.initRetirement();
          marker.childDraftId = 'grandchild-draft';
          marker.childOperation = 'grandchild-op';
          marker.operation = 'retire-middle';
          marker.parentGeneration = 1;
        },
      ),
    );
    expect(middle.record!.parentLink!.parentDraftId, 'parent-draft');
    expect(middle.record!.retirement!.childDraftId, 'grandchild-draft');
    expect(retired.record!.generation, BigInt.from(2));
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _recordFrame(
          request: parent,
          active: true,
          mutate: (record) {
            final marker = record.initRetirement();
            marker.childDraftId = 'child-draft';
            marker.childOperation = 'child-op';
            marker.operation = 'retire-op';
            marker.parentGeneration = 1;
          },
        ),
      ),
      throwsFormatException,
    );
  });

  test('lineage page checks context, order, cursor and high generations', () {
    final page = EditorDraftCodec.decodeEnvelope(_lineageFrame()).lineagePage!;
    expect(page.lineages, hasLength(2));
    expect(page.lineages.first.childGeneration, _maxU64 - BigInt.one);
    expect(page.lineages.last.childGeneration, _maxU64 - BigInt.from(2));
    expect(page.nextCursor, _secondCursor);
    expect(() => page.lineages.clear(), throwsUnsupportedError);
    expect(
      () => EditorDraftCodec.decodeEnvelope(_lineageFrame(reverse: true)),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(_lineageFrame(badContext: true)),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _lineageFrame(nextCursor: _firstCursor),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _lineageFrame(requestCursor: _secondCursor),
      ),
      throwsFormatException,
    );
  });

  test('retirement and pagination preflight reject identity drift', () {
    final request = EditorDraftRetirementRequest(
      cardId: 'card',
      childDraftId: 'child',
      parentDraftId: 'parent',
      childOperation: 'child-op',
      parentGeneration: _maxU64 - BigInt.one,
      operation: 'retire-op',
    );
    EditorDraftCodec.validateRetirement(request);
    EditorDraftCodec.validateLineagePageRequest(
      cursor: _firstCursor,
      limit: 32,
    );
    expect(
      () => EditorDraftCodec.validateRetirement(
        EditorDraftRetirementRequest(
          cardId: request.cardId,
          childDraftId: request.parentDraftId,
          parentDraftId: request.parentDraftId,
          childOperation: request.childOperation,
          parentGeneration: request.parentGeneration,
          operation: request.operation,
        ),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.validateLineagePageRequest(cursor: '../other'),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.validateLineagePageRequest(limit: 33),
      throwsFormatException,
    );
  });
}
