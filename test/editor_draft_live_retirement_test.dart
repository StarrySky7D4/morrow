import 'dart:async';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/editor_draft_retirement.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';
import 'package:morrow_studio/plugins/editor_session.dart';

import 'editor_draft_retirement_test.dart' as fixture;

final class _Editor implements WorkbenchEditorSession,
    WorkbenchEditorOperationIdentity, WorkbenchEditorDraftCaptureIdentity {
  int closes = 0;
  Completer<void>? closeHold;
  Object? closeFailure;
  @override
  String get commitOperation => 'business';
  @override
  ({String scope, int events, bool complete}) get draftCapture =>
      (scope: '', events: 0, complete: true);
  @override
  Future<void> close() async {
    closes++;
    await closeHold?.future;
    if (closeFailure case final failure?) throw failure;
  }
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

final class _Drafts implements EditorDraftControl {
  EditorDraftRecord? current;
  int discards = 0, saves = 0;
  Completer<void>? discardHold;
  Object? discardFailure;

  EditorDraftRecord make(EditorDraftWriteRequest request, {
    bool active = true, String discard = '',
  }) => EditorDraftRecord(
    request: request,
    generation: BigInt.from(active ? 1 : 2), active: active,
    currentGeneration: BigInt.from(active ? 1 : 2), currentActive: active,
    repeated: false, sourceFormat: 0, sourceRevision: BigInt.zero,
    sourceSha256: const [], predecessorRevision: BigInt.zero,
    predecessorSha256: const [], assets: const [],
    requestSha256: sha256.convert(EditorDraftCodec.encodeWrite(request)).bytes,
    discardOperation: discard,
  );
  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) async {
    saves++;
    return current = make(request);
  }
  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) async => current;
  @override
  Future<EditorDraftRecord> discard(String cardId, String draftId,
      BigInt expectedGeneration, String operation) async {
    discards++;
    await discardHold?.future;
    current = make(current!.request, active: false, discard: operation);
    if (discardFailure case final failure?) throw failure;
    return current!;
  }
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

Future<EditorDraftOwner> _ownerFor(_Drafts drafts, _Editor editor) async {
  final owner = EditorDraftOwner(
    workspace: EditorDraftWorkspace(isCurrent: () => true, capacity: 1),
    control: drafts, editor: editor,
    catalog: EditorDraftAssetCatalog.newCard(cardId: 'card', draftId: 'draft'),
    initial: EditorDraftSnapshot(values: fixture.record().request.values,
        assets: const []),
    isCurrent: () => true, operationFactory: () => 'original-save',
  );
  owner.session.pauseAutoSave();
  await owner.session.ensureJournal();
  addTearDown(owner.session.dispose);
  return owner;
}

EditorDraftRetirementAttempt _attemptFor(EditorDraftOwner owner, _Drafts drafts,
    fixture.Imports imports) {
  late EditorDraftRetirementReservation reservation;
  final original = owner.session.confirmed!;
  return EditorDraftRetirementAttempt(
    original: original, operation: 'retire-original', control: drafts,
    imports: imports, isCurrent: () => true,
    reserveOwner: () => reservation = owner.reserveRetirement(original, 'retire-original'),
    finishOwner: (record) => owner.finishRetirement(reservation, record),
  );
}

void main() {
  test('live reservation fences new work and awaits the original capture close', () async {
    final drafts = _Drafts()..discardHold = Completer<void>();
    final editor = _Editor()..closeHold = Completer<void>();
    final owner = await _ownerFor(drafts, editor);
    final imports = fixture.Imports();
    final attempt = _attemptFor(owner, drafts, imports);
    expect(owner.session.retirementFrozen, isFalse);
    final running = attempt.start();
    expect(identical(running, attempt.start()), isTrue);
    await Future<void>.delayed(Duration.zero);
    expect(drafts.discards, 1);
    expect(owner.session.retirementFrozen, isTrue);
    expect(() => owner.session.observe(owner.session.current), throwsStateError);
    expect(() => owner.session.attachUI(), throwsStateError);
    expect(() => owner.session.resumeAutoSave(), throwsStateError);
    expect(() => owner.workspace.attach(owner.session), throwsStateError);
    expect(() => owner.workspace.retain(owner.session), throwsStateError);
    expect(owner.workspace.evictDurable('card', 'draft'), isFalse);
    await expectLater(owner.session.flush(), throwsStateError);
    await expectLater(owner.session.ensureJournal(), throwsStateError);
    await expectLater(owner.workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsStateError);
    expect(owner.workspace.sessions, [owner.session]);
    expect(editor.closes, 0);
    drafts.discardHold!.complete();
    await Future<void>.delayed(Duration.zero);
    expect(owner.session.disposed, isTrue);
    expect(owner.workspace.sessions, isEmpty);
    expect(editor.closes, 1);
    expect(owner.captureClosed, isFalse);
    expect(attempt.confirmed, isNull);
    editor.closeHold!.complete();
    await running;
    expect(owner.retired, isTrue);
    expect(owner.captureClosed, isTrue);
    expect(attempt.unknown, isFalse);
    await attempt.inspect();
    expect(editor.closes, 1);
    expect(imports.reconciles, 1);
    expect(drafts.saves, 1);
  });

  test('lost discard keeps its live reservation until original readback', () async {
    final loss = StateError('original discard reply lost');
    final drafts = _Drafts()..discardFailure = loss;
    final editor = _Editor();
    final owner = await _ownerFor(drafts, editor);
    final imports = fixture.Imports();
    final attempt = _attemptFor(owner, drafts, imports);
    await expectLater(attempt.start(), throwsA(same(loss)));
    expect(owner.session.retirementFrozen, isTrue);
    expect(owner.session.disposed, isFalse);
    expect(owner.workspace.sessions, [owner.session]);
    expect(imports.reconciles, 0);
    expect(editor.closes, 0);
    await attempt.inspect();
    expect(owner.captureClosed, isTrue);
    expect(drafts.discards, 1);
    expect(imports.reconciles, 1);
  });

  test('ambiguous capture close is retained and never sent again', () async {
    final loss = StateError('original capture close reply lost');
    final drafts = _Drafts();
    final editor = _Editor()..closeFailure = loss;
    final owner = await _ownerFor(drafts, editor);
    final attempt = _attemptFor(owner, drafts, fixture.Imports());
    await expectLater(attempt.start(), throwsA(same(loss)));
    await expectLater(attempt.inspect(), throwsA(same(loss)));
    expect(editor.closes, 1);
    expect(owner.retired, isTrue);
    expect(owner.captureClosed, isFalse);
    expect(attempt.confirmed, isNull);
    expect(attempt.unknown, isTrue);
    expect(drafts.discards, 1);
  });

  test('attached and newer local input are preserved before any discard', () async {
    final drafts = _Drafts();
    final owner = await _ownerFor(drafts, _Editor());
    final lease = owner.workspace.attach(owner.session);
    await expectLater(_attemptFor(owner, drafts, fixture.Imports()).start(), throwsStateError);
    expect(owner.session.retirementFrozen, isFalse);
    lease.release();
    owner.session.observe(EditorDraftSnapshot(
      values: fixture.record(title: 'new unsaved input').request.values,
      assets: const [],
    ));
    await expectLater(_attemptFor(owner, drafts, fixture.Imports()).start(), throwsStateError);
    expect(owner.session.current.values.title.text, 'new unsaved input');
    expect(owner.session.retirementFrozen, isFalse);
    expect(drafts.discards, 0);
  });
}
