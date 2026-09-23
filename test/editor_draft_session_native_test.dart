import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available =
    Platform.isWindows && _executable != null && _package != null;

EditorDraftTextValue _text(
  String value, {
  int base = -1,
  int extent = -1,
  int composingStart = -1,
  int composingEnd = -1,
}) => EditorDraftTextValue(
  text: value,
  selectionBase: base,
  selectionExtent: extent,
  affinity: 1,
  directional: true,
  composingStart: composingStart,
  composingEnd: composingEnd,
);

EditorDraftSnapshot _snapshot(
  String title,
  String description, {
  int base = -1,
  int extent = -1,
  int composingStart = -1,
  int composingEnd = -1,
}) => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _text(title, base: title.length, extent: title.length),
    description: _text(
      description,
      base: base,
      extent: extent,
      composingStart: composingStart,
      composingEnd: composingEnd,
    ),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: '进行中',
    stage: '计划中',
  ),
  assets: const [],
);

Future<BigInt> _seed(RustWorkbench host, String id) async {
  await host.apply(
    PluginAction.create,
    Idea(
      'Formal original',
      'Formal body',
      '进行中',
      Idea.icons[0],
      const Color(0xff8866aa),
      id: id,
      stage: '计划中',
      todos: const ['keep TaskId'],
    ),
  );
  final plan = await host.versionedContent.planMigration(id);
  return (await host.versionedContent.migrate(plan)).current.revision;
}

final class _GateControl implements EditorDraftControl {
  _GateControl(this.delegate);
  final EditorDraftControl delegate;
  final firstCommitted = Completer<EditorDraftRecord>();
  final releaseFirst = Completer<void>();
  final operations = <String>[];
  bool _first = true;

  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) async {
    operations.add(request.operation);
    final gate = _first;
    _first = false;
    final record = await delegate.save(request);
    if (gate) {
      firstCommitted.complete(record);
      await releaseFirst.future;
    }
    return record;
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) =>
      delegate.read(cardId, draftId);

  @override
  Future<List<EditorDraftSummary>> list() => delegate.list();

  @override
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  ) => delegate.discard(cardId, draftId, expectedGeneration, operation);

  @override
  Future<EditorDraftImportedAsset> importAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String path,
    String name,
    String kind,
    BigInt bytes,
  ) => delegate.importAsset(
    cardId,
    draftId,
    generation,
    path,
    name,
    kind,
    bytes,
  );

  @override
  Future<void> exportAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String assetId,
    String path,
  ) => delegate.exportAsset(cardId, draftId, generation, assetId, path);
}

Future<void> _until(bool Function() ready) async {
  final deadline = DateTime.now().add(const Duration(seconds: 20));
  while (!ready()) {
    if (DateTime.now().isAfter(deadline)) {
      throw TimeoutException(
        'Editor draft session did not settle after host save',
      );
    }
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}

void main() {
  test(
    'real host session keeps S2 while S1 receipt is delayed and restores without a business edit',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-draft-session-native-',
      );
      RustWorkbench? host;
      EditorDraftSession? session;
      try {
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        const id = 'draft-session-native-card';
        const draftId = 'draft-session-native';
        final sourceRevision = await _seed(host, id);
        final gate = _GateControl(host.editorDrafts);
        var nextOperation = 0;
        session = EditorDraftSession.newSession(
          control: gate,
          cardId: id,
          draftId: draftId,
          sourceRevision: sourceRevision,
          initialSnapshot: _snapshot('baseline', 'formal local baseline'),
          operationFactory: () => 'native-session-op-${++nextOperation}',
          isCurrent: () => true,
          debounce: const Duration(milliseconds: 50),
        );
        final s1 = _snapshot(
          '',
          'A😀B',
          base: 2, // Raw UTF-16 cursor inside the surrogate pair.
          extent: 3,
          composingStart: 1,
          composingEnd: 3,
        );
        session.observe(s1);
        final first = session.flush();
        final actualS1 = await gate.firstCommitted.future;
        expect(actualS1.generation, BigInt.one);
        expect(actualS1.request.values.title.text, isEmpty);
        expect(actualS1.request.values.description.selectionBase, 2);
        expect(actualS1.request.values.description.composingStart, 1);
        expect(
          (await host.editorDrafts.read(id, draftId))!.generation,
          BigInt.one,
        );
        expect((await host.versionedContent.read(id)).revision, sourceRevision);

        final s2 = _snapshot('still raw', 'S2 😀 text', base: 5, extent: 5);
        session.observe(s2);
        expect(session.current.values.description.text, 'S2 😀 text');
        expect(session.confirmed, isNull, reason: 'S1 receipt is still gated');
        gate.releaseFirst.complete();
        await first;
        await _until(() => session!.confirmed?.generation == BigInt.two);
        expect(session.current.values.title.text, 'still raw');
        expect(
          session.confirmed!.request.values.description.text,
          'S2 😀 text',
        );
        expect(session.dirty, isFalse);
        expect(gate.operations, hasLength(2));
        expect(gate.operations[0], isNot(gate.operations[1]));
        expect((await host.versionedContent.read(id)).revision, sourceRevision);
        expect((await host.versionedContent.read(id)).title, 'Formal original');
        final frozenS1 = actualS1.request;
        session.dispose();
        session = null;
        await host.close();
        host = await RustWorkbench.open(
          executable: _executable!,
          package: _package!,
          directory: directory,
        );
        final durable = (await host.editorDrafts.read(id, draftId))!;
        expect(durable.generation, BigInt.two);
        expect(durable.request.values.description.text, 'S2 😀 text');
        final repeat = await host.editorDrafts.save(frozenS1);
        expect(repeat.repeated, isTrue);
        expect(repeat.generation, BigInt.one);
        expect(repeat.currentGeneration, BigInt.two);
        final restored = EditorDraftSession.restore(
          control: host.editorDrafts,
          record: durable,
          operationFactory: () => 'unused-restored-operation',
          isCurrent: () => true,
          debounce: const Duration(milliseconds: 50),
        );
        session = restored;
        expect(restored.current.values.description.text, 'S2 😀 text');
        expect(restored.dirty, isFalse);
        expect((await restored.flush())!.generation, BigInt.two);
        await Future<void>.delayed(const Duration(milliseconds: 120));
        expect(
          (await host.editorDrafts.read(id, draftId))!.generation,
          BigInt.two,
        );
        expect((await host.versionedContent.read(id)).revision, sourceRevision);
      } finally {
        if (session != null && !session.disposed) session.dispose();
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
