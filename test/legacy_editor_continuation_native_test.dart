import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _nativeAvailable =
    Platform.isWindows && _executable != null && _package != null;

Idea _draft(Idea source, String title) => Idea(
  title,
  source.description,
  source.category,
  source.icon,
  source.color,
  id: source.id,
  favorite: source.favorite,
  time: source.time,
  stage: source.stage,
  hypothesis: source.hypothesis,
  conclusion: source.conclusion,
  attachments: source.attachments,
  todos: source.todos,
  completed: source.completed,
);

EditorFields _fields(Idea draft) => EditorFields(
  title: draft.title,
  description: draft.description,
  hypothesis: draft.hypothesis,
  conclusion: draft.conclusion,
  todos: draft.todos.join('\n'),
);

Future<({RustWorkbench host, Directory directory, Idea original})> _seed(
  String prefix,
) async {
  final directory = await Directory.systemTemp.createTemp(prefix);
  final host = await RustWorkbench.open(
    executable: _executable!,
    package: _package!,
    directory: directory,
  );
  final original = await host.apply(
    PluginAction.create,
    Idea(
      'Seed',
      'Original body',
      '灵感',
      Idea.icons[0],
      const Color(0xff8866aa),
      id: 'legacy-continuation-card',
    ),
  );
  return (host: host, directory: directory, original: original);
}

void main() {
  test(
    'confirmed legacy S1 opens a fresh exact-revision scope for explicit S2',
    () async {
      final seeded = await _seed('morrow-legacy-continuation-');
      final host = seeded.host;
      final directory = seeded.directory;
      WorkbenchEditorSession? first;
      WorkbenchEditorSession? second;
      try {
        first = await host.openEditor(seeded.original.id, create: false);
        final firstDraft = _draft(seeded.original, 'Saved S1');
        final confirmed = await first.save(firstDraft, _fields(firstDraft));
        expect(confirmed.title, 'Saved S1');
        expect(confirmed.historicalReceipt, isFalse);
        expect(
          confirmed.contentRevision,
          seeded.original.contentRevision! + BigInt.one,
        );

        final continuation = first as WorkbenchEditorContinuation;
        final opening = continuation.continueAfterCommit(confirmed);
        expect(
          identical(opening, continuation.continueAfterCommit(confirmed)),
          isTrue,
        );
        // An exact S1 retry during the asynchronous scope handoff must not
        // replace the confirmed result that authorizes this continuation.
        await expectLater(
          first.save(firstDraft, _fields(firstDraft)),
          throwsStateError,
        );
        second = await opening;
        expect(second.targetId, seeded.original.id);
        await expectLater(
          first.save(firstDraft, _fields(firstDraft)),
          throwsStateError,
        );
        await expectLater(
          continuation.continueAfterCommit(confirmed),
          throwsStateError,
        );

        final captured = await second.studio.capture('plain', 'fresh S2 scope');
        expect(captured.ticket, isNotEmpty);
        final secondDraft = _draft(confirmed, 'Saved S2');
        final saved = await second.save(secondDraft, _fields(secondDraft));
        expect(saved.title, 'Saved S2');
        expect(saved.contentRevision, confirmed.contentRevision! + BigInt.one);
        expect((await host.load()).single.title, 'Saved S2');
      } finally {
        await second?.close();
        await first?.close();
        await host.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  test(
    'legacy continuation rejects a foreign result and a changed revision',
    () async {
      final seeded = await _seed('morrow-legacy-continuation-stale-');
      final host = seeded.host;
      final directory = seeded.directory;
      WorkbenchEditorSession? editor;
      try {
        editor = await host.openEditor(seeded.original.id, create: false);
        final firstDraft = _draft(seeded.original, 'Saved S1');
        final confirmed = await editor.save(firstDraft, _fields(firstDraft));
        final continuation = editor as WorkbenchEditorContinuation;

        final forged = _draft(confirmed, confirmed.title);
        await expectLater(
          continuation.continueAfterCommit(forged),
          throwsStateError,
        );

        final foreignDraft = _draft(confirmed, 'Foreign newer edit');
        final newer = await host.apply(PluginAction.edit, foreignDraft);
        expect(newer.contentRevision, confirmed.contentRevision! + BigInt.one);
        await expectLater(
          continuation.continueAfterCommit(confirmed),
          throwsStateError,
        );
        expect((await host.load()).single.title, 'Foreign newer edit');
        await expectLater(
          editor.save(firstDraft, _fields(firstDraft)),
          completes,
        );
      } finally {
        await editor?.close();
        await host.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_nativeAvailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
