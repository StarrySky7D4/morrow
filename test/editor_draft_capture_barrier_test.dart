import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';

EditorDraftTextValue _text(String value, {int selection = -1}) =>
    EditorDraftTextValue(
      text: value,
      selectionBase: selection,
      selectionExtent: selection,
      affinity: TextAffinity.downstream.index,
      directional: false,
      composingStart: -1,
      composingEnd: -1,
    );

EditorDraftSnapshot _snapshot(String title) => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _text(title),
    description: _text('body'),
    hypothesis: _text('hypothesis'),
    conclusion: _text('conclusion'),
    todos: _text('one\ntwo'),
    category: 'category',
    stage: 'stage',
  ),
  assets: const [],
);

EditorDraftRecord _receipt(EditorDraftWriteRequest request) {
  final generation = request.expectedGeneration + BigInt.one;
  return EditorDraftRecord(
    request: request,
    generation: generation,
    active: true,
    currentGeneration: generation,
    currentActive: true,
    repeated: false,
    sourceFormat: 2,
    sourceRevision: request.sourceRevision,
    sourceSha256: List<int>.filled(32, 7),
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: const [],
  );
}

final class _Save {
  _Save(this.request);
  final EditorDraftWriteRequest request;
  final completion = Completer<EditorDraftRecord>();
}

final class _Control implements EditorDraftControl {
  final saves = <_Save>[];

  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) {
    final item = _Save(request);
    saves.add(item);
    return item.completion.future;
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) async => null;

  @override
  Future<List<EditorDraftSummary>> list() async => const [];

  @override
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  ) => throw UnimplementedError();

  @override
  Future<EditorDraftImportedAsset> importAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String path,
    String name,
    String kind,
    BigInt bytes,
  ) => throw UnimplementedError();

  @override
  Future<void> exportAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String assetId,
    String path,
  ) => throw UnimplementedError();
}

final class _Harness {
  _Harness({Duration? debounce, String initialTitle = 'old'}) {
    session = EditorDraftSession.newSession(
      control: control,
      cardId: 'card',
      draftId: 'draft',
      sourceRevision: BigInt.one,
      initialSnapshot: _snapshot(initialTitle),
      operationFactory: () => 'save-${++sequence}',
      isCurrent: () => true,
      debounce: debounce,
    );
    controllers = [
      for (final text in [
        initialTitle,
        'body',
        'hypothesis',
        'conclusion',
        'one\ntwo',
      ])
        TextEditingController(text: text),
    ];
    workspace = EditorDraftWorkspace(isCurrent: () => true);
    lease = workspace.attach(session);
    binding = EditorDraftBinding(
      session: session,
      title: controllers[0],
      description: controllers[1],
      hypothesis: controllers[2],
      conclusion: controllers[3],
      todos: controllers[4],
      readMetadata: () {
        if (metadataError) throw metadataFailure;
        return metadata;
      },
      isCurrent: () => true,
    )..attach();
  }

  final control = _Control();
  final metadataFailure = StateError('Selected attachment unresolved');
  late final EditorDraftWorkspace workspace;
  late final EditorDraftViewLease lease;
  late final EditorDraftSession session;
  late final EditorDraftBinding binding;
  late final List<TextEditingController> controllers;
  EditorDraftMetadata metadata = EditorDraftMetadata(
    category: 'category',
    stage: 'stage',
    assets: const [],
  );
  bool metadataError = false;
  int sequence = 0;

  Future<EditorDraftRecord> confirmInitial() async {
    final saving = session.ensureJournal();
    await _tick();
    final request = control.saves.single.request;
    final record = _receipt(request);
    control.saves.single.completion.complete(record);
    expect(await saving, same(record));
    return record;
  }

  void dispose() {
    binding.dispose();
    lease.release();
    for (final controller in controllers) {
      controller.dispose();
    }
    session.dispose();
  }
}

Future<void> _tick() => Future<void>.delayed(Duration.zero);

void main() {
  test('metadata failure cancels an already queued debounce', () async {
    final h = _Harness(debounce: const Duration(milliseconds: 20));
    addTearDown(h.dispose);
    h.controllers[0].text = 'captured S1';
    expect(h.session.current.values.title.text, 'captured S1');
    h.metadataError = true;
    h.controllers[0].text = 'raw S2';
    expect(h.binding.hasUncapturedChanges, isTrue);
    expect(h.session.captureBlocked, isTrue);
    expect(h.session.captureFailure, same(h.metadataFailure));
    expect(h.session.current.values.title.text, 'captured S1');
    expect(h.session.dirty, isTrue);
    await Future<void>.delayed(const Duration(milliseconds: 60));
    expect(h.control.saves, isEmpty);
    await expectLater(h.session.flush(), throwsStateError);
    await expectLater(h.session.ensureJournal(), throwsStateError);
    await expectLater(
      h.session.flushLatestVisible(timeout: const Duration(seconds: 1)),
      throwsStateError,
    );
    expect(h.control.saves, isEmpty);
  });

  test(
    'confirmed old value cannot close, evict, or reuse a close proof',
    () async {
      final h = _Harness(debounce: null);
      addTearDown(h.dispose);
      final confirmed = await h.confirmInitial();
      expect(h.session.dirty, isFalse);
      final prepared = await h.workspace.prepareClose(
        timeout: const Duration(seconds: 1),
      );
      final before = h.session.localGeneration;
      h.metadataError = true;
      expect(h.binding.capture(), isFalse);
      expect(h.session.localGeneration, greaterThan(before));
      expect(h.session.captureBlocked, isTrue);
      h.binding.detach();
      h.lease.release();
      expect(() => h.workspace.finishClose(prepared), throwsStateError);
      h.workspace.cancelClose(prepared);
      expect(h.session.captureBlocked, isTrue);
      h.session.pauseAutoSave();
      h.session.resumeAutoSave();
      h.session.adoptConfirmedAssetPins(confirmed);
      expect(h.session.captureBlocked, isTrue);
      expect(h.session.dirty, isTrue);
      expect(h.workspace.evictDurable('card', 'draft'), isFalse);
      await expectLater(
        h.workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsStateError,
      );
      expect(h.workspace.preparingClose, isFalse);

      // A complete equal observation is enough to restore trust in S1.
      h.metadataError = false;
      h.binding.attach();
      expect(h.binding.capture(), isTrue);
      expect(h.session.captureBlocked, isFalse);
      expect(h.session.dirty, isFalse);
      expect(h.control.saves, hasLength(1));
    },
  );

  test(
    'S1 acknowledgement after a failed capture cannot save or close S2',
    () async {
      final h = _Harness(debounce: const Duration(milliseconds: 10));
      addTearDown(h.dispose);
      h.controllers[0].text = 'S1';
      final first = h.session.flush();
      await _tick();
      final original = h.control.saves.single.request;
      h.metadataError = true;
      h.controllers[0].text = 'raw S2';
      final failedGeneration = h.session.localGeneration;
      h.control.saves.single.completion.complete(_receipt(original));
      await first;
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(h.session.confirmed!.request, same(original));
      expect(h.session.captureBlocked, isTrue);
      expect(h.session.localGeneration, failedGeneration);
      expect(h.session.dirty, isTrue);
      expect(h.control.saves, hasLength(1));
      await expectLater(
        h.workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsStateError,
      );
      h.binding.detach();
      h.lease.release();
      expect(h.workspace.evictDurable('card', 'draft'), isFalse);
    },
  );

  test(
    'unknown S1 may retry exactly, but reconciliation keeps capture blocked',
    () async {
      final h = _Harness(debounce: null);
      addTearDown(h.dispose);
      h.controllers[0].text = 'S1';
      final first = h.session.flush();
      await _tick();
      final original = h.control.saves.single.request;
      final encoded = EditorDraftCodec.encodeWrite(original);
      h.metadataError = true;
      h.controllers[0].text = 'raw S2';
      final unknown = EditorDraftSaveFailure(
        request: original,
        outcomeUnknown: true,
        cause: StateError('reply lost'),
      );
      h.control.saves.single.completion.completeError(unknown);
      await expectLater(first, throwsA(same(unknown)));
      expect(h.session.unknown, isTrue);
      expect(h.session.captureBlocked, isTrue);
      final retry = h.session.retryPending();
      await _tick();
      expect(h.control.saves, hasLength(2));
      expect(
        EditorDraftCodec.encodeWrite(h.control.saves.last.request),
        encoded,
      );
      h.control.saves.last.completion.complete(_receipt(original));
      await retry;
      expect(h.session.unknown, isFalse);
      expect(h.session.captureBlocked, isTrue);
      expect(h.session.current.values.title.text, 'S1');
      expect(h.session.dirty, isTrue);
      await expectLater(h.session.flush(), throwsStateError);
      await expectLater(
        h.workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsStateError,
      );
      expect(h.control.saves, hasLength(2));
    },
  );

  test(
    'successful recapture saves raw text, selection, IME and metadata',
    () async {
      final h = _Harness(debounce: null);
      addTearDown(h.dispose);
      final confirmed = await h.confirmInitial();
      h.metadataError = true;
      h.controllers[0].value = const TextEditingValue(
        text: 'A😀B',
        selection: TextSelection(
          baseOffset: 3,
          extentOffset: 1,
          affinity: TextAffinity.upstream,
          isDirectional: true,
        ),
        composing: TextRange(start: 1, end: 3),
      );
      h.controllers[1].text = 'raw description';
      h.controllers[2].text = 'raw hypothesis';
      h.controllers[3].text = 'raw conclusion';
      h.controllers[4].text = 'raw todo';
      expect(h.session.captureBlocked, isTrue);
      expect(h.session.confirmed, same(confirmed));
      h.metadataError = false;
      h.metadata = EditorDraftMetadata(
        category: 'repaired category',
        stage: 'repaired stage',
        assets: const [],
      );
      expect(h.binding.capture(), isTrue);
      expect(h.session.captureBlocked, isFalse);
      final second = h.session.flush();
      await _tick();
      expect(h.control.saves, hasLength(2));
      final request = h.control.saves.last.request;
      expect(request.values.title.text, 'A😀B');
      expect(request.values.title.selectionBase, 3);
      expect(request.values.title.selectionExtent, 1);
      expect(request.values.title.affinity, TextAffinity.upstream.index);
      expect(request.values.title.directional, isTrue);
      expect(request.values.title.composingStart, 1);
      expect(request.values.title.composingEnd, 3);
      expect(request.values.description.text, 'raw description');
      expect(request.values.hypothesis.text, 'raw hypothesis');
      expect(request.values.conclusion.text, 'raw conclusion');
      expect(request.values.todos.text, 'raw todo');
      expect(request.values.category, 'repaired category');
      expect(request.values.stage, 'repaired stage');
      h.control.saves.last.completion.complete(_receipt(request));
      await second;
      expect(h.session.dirty, isFalse);
    },
  );

  test('failed restore cannot clear an existing capture barrier', () async {
    final h = _Harness(debounce: null);
    addTearDown(h.dispose);
    await h.confirmInitial();
    h.metadataError = true;
    h.controllers[0].text = 'raw unsaved';
    expect(h.session.captureBlocked, isTrue);
    final restoreFailure = StateError('metadata restore failed');
    expect(
      () => h.binding.applyCurrentToView((_) => throw restoreFailure),
      throwsA(same(restoreFailure)),
    );
    expect(h.session.captureBlocked, isTrue);
    expect(h.session.dirty, isTrue);
    expect(h.controllers[0].text, 'raw unsaved');
    await expectLater(h.session.flush(), throwsStateError);
    expect(h.control.saves, hasLength(1));
  });
}
