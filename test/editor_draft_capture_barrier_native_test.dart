import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_codec.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

EditorDraftTextValue _emptyText() => EditorDraftTextValue(
  text: '',
  selectionBase: -1,
  selectionExtent: -1,
  affinity: TextAffinity.downstream.index,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftSnapshot _initial() => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _emptyText(),
    description: _emptyText(),
    hypothesis: _emptyText(),
    conclusion: _emptyText(),
    todos: _emptyText(),
    category: '',
    stage: '',
  ),
  assets: const [],
);

void main() {
  test(
    'native new-card capture barrier preserves S1 and persists complete S2 after restart',
    () async {
      final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
      final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
      expect(Platform.isWindows, isTrue);
      expect(executable, isNotNull, reason: 'Set MORROW_WORKBENCH_HOST');
      expect(package, isNotNull, reason: 'Set MORROW_WORKBENCH_PACKAGE');
      expect(await File(executable!).exists(), isTrue);
      expect(await File(package!).exists(), isTrue);

      final directory = await Directory.systemTemp.createTemp(
        'morrow-capture-barrier-native-',
      );
      RustWorkbench? host;
      EditorDraftSession? session;
      EditorDraftBinding? binding;
      EditorDraftViewLease? lease;
      final controllers = [
        TextEditingController(text: '  S1 raw \n'),
        TextEditingController(text: 'S1 body'),
        TextEditingController(text: 'S1 hypothesis'),
        TextEditingController(text: 'S1 conclusion'),
        TextEditingController(text: 'S1 todo'),
      ];
      var metadataError = false;
      final metadataFailure = StateError('attachment selection unresolved');
      var metadata = EditorDraftMetadata(
        category: 'S1 category',
        stage: 'S1 stage',
        assets: const [],
      );
      var serial = 0;
      const cardId = 'capture-barrier-new-card';
      const draftId = 'capture-barrier-draft';
      try {
        host = await RustWorkbench.open(
          executable: executable,
          package: package,
          directory: directory,
        );
        expect(await host.loadWorkspaceContent(), isEmpty);
        expect(await host.editorDrafts.read(cardId, draftId), isNull);

        session = EditorDraftSession.newSession(
          control: host.editorDrafts,
          cardId: cardId,
          draftId: draftId,
          sourceKind: EditorDraftSourceKind.newCard,
          sourceRevision: BigInt.zero,
          initialSnapshot: _initial(),
          operationFactory: () => 'native-barrier-${++serial}',
          isCurrent: () => true,
          debounce: const Duration(milliseconds: 20),
        );
        final workspace = EditorDraftWorkspace(isCurrent: () => true);
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

        expect(binding.capture(), isTrue);
        final first = (await session.flush())!;
        expect(first.generation, BigInt.one);
        expect(first.request.sourceKind, EditorDraftSourceKind.newCard);
        expect(first.request.values.title.text, '  S1 raw \n');
        expect(serial, 1);
        final before = (await host.editorDrafts.read(cardId, draftId))!;
        final beforeBytes = EditorDraftCodec.encodeWrite(before.request);

        metadataError = true;
        controllers[0].value = const TextEditingValue(
          text: '  S2😀原文 \n',
          selection: TextSelection(
            baseOffset: 6,
            extentOffset: 2,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: TextRange(start: 2, end: 6),
        );
        controllers[1].text = ' S2 body \n';
        controllers[2].text = 'S2 hypothesis 😀';
        controllers[3].text = 'S2 conclusion';
        controllers[4].text = '  S2 todo \n';
        expect(binding.hasUncapturedChanges, isTrue);
        expect(binding.captureFailure, same(metadataFailure));
        expect(session.captureBlocked, isTrue);
        expect(session.current.values.title.text, '  S1 raw \n');
        await expectLater(session.flush(), throwsStateError);
        await expectLater(
          workspace.prepareClose(timeout: const Duration(seconds: 2)),
          throwsStateError,
        );
        await Future<void>.delayed(const Duration(milliseconds: 60));
        expect(serial, 1);
        final stillS1 = (await host.editorDrafts.read(cardId, draftId))!;
        expect(stillS1.generation, BigInt.one);
        expect(stillS1.request.operation, first.request.operation);
        expect(EditorDraftCodec.encodeWrite(stillS1.request), beforeBytes);
        expect(stillS1.request.values.title.text, '  S1 raw \n');
        expect(stillS1.request.values.description.text, 'S1 body');
        expect(await host.loadWorkspaceContent(), isEmpty);

        metadataError = false;
        metadata = EditorDraftMetadata(
          category: ' 分类\u00a0原样 ',
          stage: ' 阶段\u0085原样 ',
          assets: const [],
        );
        expect(binding.capture(), isTrue);
        expect(session.captureBlocked, isFalse);
        final second = (await session.flush())!;
        expect(second.generation, BigInt.two);
        expect(second.request.operation, isNot(first.request.operation));
        expect(serial, 2);

        binding.dispose();
        binding = null;
        lease.release();
        lease = null;
        session.dispose();
        session = null;
        await host.close();
        host = await RustWorkbench.open(
          executable: executable,
          package: package,
          directory: directory,
        );
        final restored = (await host.editorDrafts.read(cardId, draftId))!;
        expect(restored.generation, BigInt.two);
        expect(restored.request.operation, second.request.operation);
        expect(restored.request.sourceKind, EditorDraftSourceKind.newCard);
        final values = restored.request.values;
        expect(values.title.text, '  S2😀原文 \n');
        expect(values.title.selectionBase, 6);
        expect(values.title.selectionExtent, 2);
        expect(values.title.affinity, TextAffinity.upstream.index);
        expect(values.title.directional, isTrue);
        expect(values.title.composingStart, 2);
        expect(values.title.composingEnd, 6);
        expect(values.description.text, ' S2 body \n');
        expect(values.hypothesis.text, 'S2 hypothesis 😀');
        expect(values.conclusion.text, 'S2 conclusion');
        expect(values.todos.text, '  S2 todo \n');
        expect(values.category, ' 分类\u00a0原样 ');
        expect(values.stage, ' 阶段\u0085原样 ');
        expect(await host.loadWorkspaceContent(), isEmpty);
      } finally {
        binding?.dispose();
        lease?.release();
        session?.dispose();
        for (final controller in controllers) {
          controller.dispose();
        }
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
