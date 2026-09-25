import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/generated/editor_draft_api.capnp.dart'
    as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;

final _maxU64 = (BigInt.one << 64) - BigInt.one;
final _cursor1 = "morrow-host-editor-handoff-proposal-${'1' * 64}";
final _cursor2 = "morrow-host-editor-handoff-proposal-${'2' * 64}";

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: value.length,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftHandoffProposal _proposal({
  String child = 'child',
  String operation = 'child-op',
  String retirement = 'retire-op',
  BigInt? sourceRevision,
}) => EditorDraftHandoffProposal(
  handoff: EditorDraftHandoffRequest(
    request: EditorDraftWriteRequest(
      cardId: 'card',
      draftId: child,
      operation: operation,
      expectedGeneration: BigInt.zero,
      sourceRevision: sourceRevision ?? BigInt.one,
      predecessorOperation: '',
      predecessorDigest: const [],
      values: EditorDraftValues(
        title: _text('A😀'),
        description: _text(''),
        hypothesis: _text(''),
        conclusion: _text(''),
        todos: _text(''),
        category: '',
        stage: '',
      ),
      assets: const [],
    ),
    parentLink: EditorDraftParentLink(
      parentDraftId: 'parent',
      parentGeneration: BigInt.one,
      parentSaveOperation: 'parent-op',
      parentRequestSha256: List<int>.filled(32, 7),
      committedOperation: 'commit-op',
      committedSha256: List<int>.filled(32, 8),
      childOperation: operation,
    ),
  ),
  retirementOperation: retirement,
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

void _writeProposal(
  wire.HandoffProposalBuilder out,
  EditorDraftHandoffProposal proposal,
) {
  final request = proposal.handoff.request;
  final link = proposal.handoff.parentLink;
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  final wr = out.initRequest();
  wr.version = 1;
  wr.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  wr.cardId = request.cardId;
  wr.draftId = request.draftId;
  wr.operation = request.operation;
  wr.expectedGeneration = 0;
  wr.sourceRevisionBigInt = request.sourceRevision;
  wr.sourceKind = request.sourceKind.index;
  wr.predecessorOperation = '';
  wr.predecessorDigest = Uint8List(0);
  final values = wr.initValues();
  _writeText(values.initTitle(), request.values.title);
  _writeText(values.initDescription(), request.values.description);
  _writeText(values.initHypothesis(), request.values.hypothesis);
  _writeText(values.initConclusion(), request.values.conclusion);
  _writeText(values.initTodos(), request.values.todos);
  values.category = '';
  values.stage = '';
  wr.initAssets(0);
  final parent = out.initParentLink();
  parent.parentDraftId = link.parentDraftId;
  parent.parentGenerationBigInt = link.parentGeneration;
  parent.parentSaveOperation = link.parentSaveOperation;
  parent.parentRequestSha256 = Uint8List.fromList(link.parentRequestSha256);
  parent.committedOperation = link.committedOperation;
  parent.committedSha256 = Uint8List.fromList(link.committedSha256);
  parent.childOperation = link.childOperation;
  out.retirementOperation = proposal.retirementOperation;
}

void _writeSummary(
  wire.HandoffProposalSummaryBuilder out, {
  String child = 'child',
  String operation = 'child-op',
  String retirement = 'retire-op',
  String? cursor,
  int status = 2,
  int revision = 1,
  int parentGeneration = 2,
  bool parentActive = false,
  BigInt? childGeneration,
  bool childActive = false,
}) {
  out.cardId = 'card';
  out.parentDraftId = 'parent';
  out.childDraftId = child;
  out.childOperation = operation;
  out.retirementOperation = retirement;
  out.revision = revision;
  out.status = status;
  out.parentGeneration = parentGeneration;
  out.parentActive = parentActive;
  out.childGenerationBigInt = childGeneration ?? _maxU64;
  out.childActive = childActive;
  out.cursor = cursor ?? _cursor1;
}

Uint8List _recordFrame({
  void Function(wire.HandoffProposalSummaryBuilder)? mutate,
  String outerOperation = 'child-op',
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.handoffProposal;
  out.cardId = 'card';
  out.draftId = 'parent';
  out.operation = outerOperation;
  out.expectedGeneration = 0;
  final record = out.initHandoffProposal();
  _writeProposal(record.initProposal(), _proposal());
  final summary = record.initSummary();
  _writeSummary(summary);
  mutate?.call(summary);
  return message.serialize();
}

Uint8List _pageFrame({
  int count = 2,
  String requestCursor = '',
  String? nextCursor,
  bool reverse = false,
  bool duplicate = false,
  bool extraRecord = false,
  bool extraLineages = false,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.handoffProposals;
  out.cardId = '';
  out.draftId = '';
  out.operation = '';
  out.expectedGeneration = 0;
  out.requestCursor = requestCursor;
  out.requestLimit = 2;
  out.nextCursor = nextCursor ?? (count == 0 ? '' : _cursor2);
  if (extraRecord) out.initHandoffProposal();
  if (extraLineages) out.initLineages(0);
  final rows = out.initHandoffProposalSummaries(count);
  for (var i = 0; i < count; i++) {
    _writeSummary(
      rows[i],
      child: duplicate ? 'child' : 'child-$i',
      operation: duplicate ? 'child-op' : 'child-op-$i',
      cursor: i == 0
          ? (reverse ? _cursor2 : _cursor1)
          : (reverse ? _cursor1 : _cursor2),
    );
  }
  return message.serialize();
}

Uint8List _absentFrame({
  String operation = 'child-op',
  int expectedGeneration = 0,
  bool unexpectedProposal = false,
}) {
  final message = MessageBuilder();
  final out = message.initRoot(wire.envelopeFactory);
  out.version = 1;
  out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
  out.kind = wire.ResultKind.absent;
  out.cardId = 'card';
  out.draftId = 'parent';
  out.operation = operation;
  out.expectedGeneration = expectedGeneration;
  if (unexpectedProposal) out.initHandoffProposal();
  return message.serialize();
}

void main() {
  test('absent proposal inspect retains a validated child operation', () {
    final absent = EditorDraftCodec.decodeEnvelope(_absentFrame());
    expect(absent.kind, EditorDraftResultKind.absent);
    expect(absent.cardId, 'card');
    expect(absent.draftId, 'parent');
    expect(absent.operation, 'child-op');
    expect(absent.expectedGeneration, BigInt.zero);
    expect(absent.handoffProposal, isNull);
    expect(
      EditorDraftCodec.decodeEnvelope(_absentFrame(operation: '')).operation,
      isEmpty,
    );
    for (final frame in [
      _absentFrame(operation: 'bad/name'),
      _absentFrame(expectedGeneration: 1),
      _absentFrame(unexpectedProposal: true),
    ]) {
      expect(
        () => EditorDraftCodec.decodeEnvelope(frame),
        throwsFormatException,
      );
    }
  });

  test('proposal freezes evidence and preserves UTF-16 and high u64', () {
    final proposal = _proposal(sourceRevision: _maxU64);
    final bytes = EditorDraftCodec.encodeHandoffProposal(proposal);
    final decoded = EditorDraftCodec.decodeHandoffProposal(bytes);
    expect(decoded.handoff.request.sourceRevision, _maxU64);
    expect(decoded.handoff.request.values.title.text, 'A😀');
    expect(decoded.handoff.request.values.title.selectionBase, 3);
    expect(decoded.retirementOperation, 'retire-op');
    expect(
      () => decoded.handoff.parentLink.committedSha256[0] = 0,
      throwsUnsupportedError,
    );
    expect(bytes.length, lessThan(EditorDraftCodec.maxFrameBytes));
  });

  test('proposal rejects identity, operation and digest drift', () {
    expect(
      () => EditorDraftCodec.encodeHandoffProposal(
        _proposal(retirement: 'child-op'),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.encodeHandoffProposal(
        _proposal(retirement: 'bad/name'),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.validateHandoffProposalIdentity(
        'card',
        'parent',
        r'bad\operation',
      ),
      throwsFormatException,
    );
    final message = MessageBuilder();
    final root = message.initRoot(wire.handoffProposalFactory);
    _writeProposal(root, _proposal());
    root.digest = Uint8List(32);
    expect(
      () => EditorDraftCodec.decodeHandoffProposal(message.serialize()),
      throwsFormatException,
    );
  });

  test('record binds full proposal and accepts inactive historical child', () {
    final result = EditorDraftCodec.decodeEnvelope(_recordFrame());
    expect(result.kind, EditorDraftResultKind.handoffProposal);
    expect(
      result.handoffProposal!.summary.status,
      EditorDraftHandoffProposalStatus.parentRetired,
    );
    expect(result.handoffProposal!.summary.childGeneration, _maxU64);
    expect(result.handoffProposal!.summary.childActive, isFalse);
    expect(result.handoffProposal!.proposal.retirementOperation, 'retire-op');
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _recordFrame(outerOperation: 'other-op'),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _recordFrame(mutate: (row) => row.retirementOperation = 'other'),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _recordFrame(mutate: (row) => row.status = 5),
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.decodeEnvelope(
        _recordFrame(mutate: (row) => row.revision = 0),
      ),
      throwsFormatException,
    );
  });

  test('proposal page bounds, cursor progress and identities', () {
    final page = EditorDraftCodec.decodeEnvelope(
      _pageFrame(),
    ).handoffProposalPage!;
    expect(page.proposals, hasLength(2));
    expect(page.nextCursor, _cursor2);
    expect(() => page.proposals.clear(), throwsUnsupportedError);
    for (final frame in [
      _pageFrame(reverse: true),
      _pageFrame(duplicate: true),
      _pageFrame(requestCursor: _cursor2),
      _pageFrame(nextCursor: _cursor1),
      _pageFrame(extraRecord: true),
      _pageFrame(extraLineages: true),
    ]) {
      expect(
        () => EditorDraftCodec.decodeEnvelope(frame),
        throwsFormatException,
      );
    }
    EditorDraftCodec.validateHandoffProposalPageRequest(
      cursor: _cursor1,
      limit: 32,
    );
    expect(
      () => EditorDraftCodec.validateHandoffProposalPageRequest(
        cursor: '../bad',
        limit: 1,
      ),
      throwsFormatException,
    );
    expect(
      () => EditorDraftCodec.validateHandoffProposalPageRequest(
        cursor: '',
        limit: 33,
      ),
      throwsFormatException,
    );
  });
}
