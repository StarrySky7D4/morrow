import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_retirement.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';

EditorDraftTextValue text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftRecord record({
  bool active = true,
  String discard = '',
  int gen = 0,
  String title = '',
}) {
  final request = EditorDraftWriteRequest(
    cardId: 'card',
    draftId: 'draft',
    operation: 'original-save',
    expectedGeneration: BigInt.zero,
    sourceRevision: BigInt.zero,
    sourceKind: EditorDraftSourceKind.newCard,
    commitOperation: 'business',
    predecessorOperation: '',
    predecessorDigest: const [],
    assets: const [],
    values: EditorDraftValues(
      title: text(title),
      description: text('x馃榾y'),
      hypothesis: text(''),
      conclusion: text(''),
      todos: text(''),
      category: 'experiment',
      stage: 'draft',
    ),
  );
  final generation = BigInt.from(gen == 0 ? (active ? 1 : 2) : gen);
  return EditorDraftRecord(
    request: request,
    generation: generation,
    active: active,
    currentGeneration: generation,
    currentActive: active,
    repeated: false,
    sourceFormat: 0,
    sourceRevision: BigInt.zero,
    sourceSha256: const [],
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: const [],
    requestSha256: List.filled(32, 5),
    discardOperation: discard,
  );
}

final class Drafts implements EditorDraftControl {
  EditorDraftRecord? current = record();
  int discards = 0;
  Object? replyFailure;
  Completer<void>? hold;
  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) async =>
      current;
  @override
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  ) async {
    discards++;
    await hold?.future;
    current = record(active: false, discard: operation);
    if (replyFailure != null) throw replyFailure!;
    return current!;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

final class Imports implements EditorDraftImportControl {
  int reconciles = 0;
  Object? replyFailure;
  bool closed = false, wrongGeneration = false;
  EditorDraftImportSnapshot snapshot(EditorDraftImportResultKind kind) =>
      EditorDraftImportSnapshot(
        kind: kind,
        cardId: 'card',
        draftId: 'draft',
        operation: '',
        importOperation: '',
        expectedGeneration: BigInt.zero,
        currentGeneration: BigInt.from(wrongGeneration ? 3 : 2),
        mainActive: false,
        stagingRevision: BigInt.one,
        records: const [],
        exportBytes: BigInt.zero,
      );
  @override
  Future<EditorDraftImportSnapshot> list(String cardId, String draftId) async {
    if (!closed) throw StateError('Imports remain owned');
    return snapshot(EditorDraftImportResultKind.list);
  }

  @override
  Future<EditorDraftImportSnapshot> reconcile(
    String cardId,
    String draftId,
  ) async {
    reconciles++;
    closed = true;
    if (replyFailure != null) throw replyFailure!;
    return snapshot(EditorDraftImportResultKind.reconciled);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test(
    'lost import cleanup reply requires original readback and no second cleanup',
    () async {
      final loss = StateError('cleanup reply lost');
      final drafts = Drafts();
      final imports = Imports()..replyFailure = loss;
      final attempt = EditorDraftRetirementAttempt(
        original: record(),
        operation: 'retire-one',
        control: drafts,
        imports: imports,
        isCurrent: () => true,
      );
      await expectLater(attempt.start(), throwsA(same(loss)));
      expect(attempt.unknown, isTrue);
      imports.wrongGeneration = true;
      await expectLater(attempt.inspect(), throwsStateError);
      expect(attempt.confirmed, isNull);
      imports.wrongGeneration = false;
      await attempt.inspect();
      expect(imports.reconciles, 1);
      expect(drafts.discards, 1);
      expect(attempt.firstFailure, same(loss));
    },
  );
  test('concurrent start consumes the original discard exactly once', () async {
    final drafts = Drafts()..hold = Completer<void>();
    final attempt = EditorDraftRetirementAttempt(
      original: record(),
      operation: 'retire-one',
      control: drafts,
      imports: Imports(),
      isCurrent: () => true,
    );
    final first = attempt.start();
    final second = attempt.start();
    expect(identical(first, second), isTrue);
    await Future<void>.delayed(Duration.zero);
    expect(drafts.discards, 1);
    drafts.hold!.complete();
    expect((await first).discardOperation, 'retire-one');
    expect(identical(await second, attempt.confirmed), isTrue);
    await attempt.inspect();
    expect(drafts.discards, 1);
    expect(attempt.unknown, isFalse);
  });

  test(
    'lost reply is resolved by reading its durable marker without replay',
    () async {
      final loss = StateError('original reply lost');
      final drafts = Drafts()..replyFailure = loss;
      final attempt = EditorDraftRetirementAttempt(
        original: record(),
        operation: 'retire-one',
        control: drafts,
        imports: Imports(),
        isCurrent: () => true,
      );
      await expectLater(attempt.start(), throwsA(same(loss)));
      expect(attempt.unknown, isTrue);
      await expectLater(attempt.start(), throwsA(same(loss)));
      expect((await attempt.inspect()).discardOperation, 'retire-one');
      expect(attempt.firstFailure, same(loss));
      expect(drafts.discards, 1);
    },
  );

  for (final bad in <String, EditorDraftRecord?>{
    'absent': null,
    'still active': record(),
    'legacy inactive': record(active: false),
    'another discard': record(active: false, discard: 'someone-else'),
    'later generation': record(active: false, discard: 'retire-one', gen: 3),
    'changed request': record(
      active: false,
      discard: 'retire-one',
      title: 'changed',
    ),
  }.entries) {
    test('${bad.key} cannot resolve a lost original retirement', () async {
      final loss = StateError('lost');
      final drafts = Drafts()..replyFailure = loss;
      final attempt = EditorDraftRetirementAttempt(
        original: record(),
        operation: 'retire-one',
        control: drafts,
        imports: Imports(),
        isCurrent: () => true,
      );
      await expectLater(attempt.start(), throwsA(same(loss)));
      drafts.current = bad.value;
      await expectLater(attempt.inspect(), throwsStateError);
      expect(attempt.confirmed, isNull);
      expect(attempt.firstFailure, same(loss));
      expect(attempt.unknown, isTrue);
      expect(drafts.discards, 1);
    });
  }

  test('workspace or generation change prevents the first mutation', () async {
    for (final workspaceChanged in [false, true]) {
      final drafts = Drafts();
      if (!workspaceChanged) drafts.current = record(gen: 2);
      final attempt = EditorDraftRetirementAttempt(
        original: record(),
        operation: 'retire-one',
        control: drafts,
        imports: Imports(),
        isCurrent: () => !workspaceChanged,
      );
      await expectLater(attempt.start(), throwsStateError);
      await expectLater(attempt.inspect(), throwsStateError);
      expect(drafts.discards, 0);
    }
  });
}
