import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_editor.dart';
import 'package:morrow_studio/plugins/versioned_editor_adapter.dart';
import 'package:morrow_studio/plugins/versioned_idea_view.dart';

VersionedContentRecord record({
  BigInt? revision,
  List<VersionedAsset> assets = const [],
}) => VersionedContentRecord(
  id: 'card-v2',
  title: 'Before',
  revision: revision ?? BigInt.from(7),
  formatVersion: 2,
  description: 'Before body',
  category: '进行中',
  stage: '计划中',
  hypothesis: '',
  conclusion: '',
  favorite: true,
  assets: assets,
  icon: 0,
  color: 0xff8866aa,
  deleted: false,
  deletedAt: BigInt.zero,
  todos: const [],
  completed: const [],
  tasks: const [],
  origin: null,
  retiredTaskIds: const [],
  projectedStage: '计划中',
  completeCount: 0,
  incompleteCount: 0,
  ambiguousCount: 0,
);

Idea draft(
  VersionedIdeaView source, {
  List<IdeaAttachment> attachments = const [],
  String title = 'Edited',
  String description = 'Edited body',
  String? category,
  String? stage,
  bool? favorite,
  List<String> todos = const [],
}) => Idea(
  title,
  description,
  category ?? source.category,
  Idea.icons[source.iconIndex],
  Color(source.colorArgb),
  id: source.id,
  stage: stage ?? source.stage,
  favorite: favorite ?? source.favorite,
  hypothesis: '',
  conclusion: '',
  attachments: attachments,
  todos: todos,
  contentRevision: source.revision,
  versioned: source,
);

EditorFields editor({
  String title = ' Edited ',
  String description = 'Edited body',
}) => EditorFields(
  title: title,
  description: description,
  hypothesis: '',
  conclusion: '',
  todos: '',
);

Idea presented(VersionedContentRecord current) {
  final view = VersionedIdeaView.fromRecord(current);
  return draft(view, title: current.title, description: current.description);
}

class FakeStudio implements StudioBackend {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class FakeSession implements VersionedEditorSession {
  final FakeStudio fakeStudio = FakeStudio();
  Future<VersionedAsset> Function(IdeaAttachment)? onStage;
  Future<VersionedMutationResult> Function(CardEditFields, EditorFields)?
  onSave;
  int stages = 0, saves = 0, pastes = 0, closes = 0;
  CardEditFields? lastFields;
  EditorFields? lastEditor;

  @override
  String get targetId => 'card-v2';
  @override
  StudioBackend get studio => fakeStudio;
  @override
  Future<void> recordPaste(PasteInsertion _) async {
    pastes++;
  }

  @override
  Future<VersionedAsset> stageAttachment(IdeaAttachment value) {
    stages++;
    return onStage!(value);
  }

  @override
  Future<VersionedMutationResult> save(
    CardEditFields fields,
    EditorFields editor,
  ) {
    saves++;
    lastFields = fields;
    lastEditor = editor;
    return onSave!(fields, editor);
  }

  @override
  Future<void> close() async {
    closes++;
  }
}

VersionedMutationResult result(VersionedContentRecord current) =>
    VersionedMutationResult(
      receipt: VersionedCommitReceipt(
        id: current.id,
        operation: 'same-operation',
        revision: current.revision,
        repeated: false,
      ),
      current: current,
    );

class AcknowledgingSession extends FakeSession
    implements VersionedEditorAcknowledgement {
  final acknowledgements = <VersionedCommitReceipt>[];
  Future<void> Function()? onAcknowledge;
  @override
  Future<void> acknowledgePresented(VersionedCommitReceipt receipt) async {
    acknowledgements.add(receipt);
    await onAcknowledge?.call();
  }
}

void main() {
  test(
    'workspace switch during presentation retains durable proposal',
    () async {
      final source = VersionedIdeaView.fromRecord(record());
      final fake = AcknowledgingSession();
      fake.onSave = (_, _) async => result(record(revision: BigInt.from(8)));
      final waiting = Completer<Idea>();
      final entered = Completer<void>();
      var current = true;
      final adapter = VersionedWorkbenchEditorAdapter(fake, source, (_) {
        entered.complete();
        return waiting.future;
      }, isCurrent: () => current);
      final future = adapter.save(draft(source), editor());
      final rejected = expectLater(
        future,
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      await entered.future;
      current = false;
      waiting.complete(presented(record(revision: BigInt.from(8))));
      await rejected;
      expect(fake.acknowledgements, isEmpty);
      await expectLater(
        adapter.save(draft(source), editor()),
        throwsStateError,
      );
      expect(fake.saves, 1);
    },
  );

  test(
    'durable proposal is acknowledged only after validated presentation',
    () async {
      final source = VersionedIdeaView.fromRecord(record());
      final fake = AcknowledgingSession();
      fake.onSave = (_, _) async => result(record(revision: BigInt.from(8)));
      var presentationFails = true;
      final events = <String>[];
      fake.onAcknowledge = () async => events.add('ack');
      final adapter = VersionedWorkbenchEditorAdapter(fake, source, (
        current,
      ) async {
        events.add('present');
        if (presentationFails) {
          throw StateError('attachment export unavailable');
        }
        return presented(current);
      });
      await expectLater(
        adapter.save(draft(source), editor()),
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      expect(fake.acknowledgements, isEmpty);
      presentationFails = false;
      await adapter.save(draft(source), editor());
      expect(events, ['present', 'present', 'ack']);
      expect(fake.acknowledgements.single.operation, 'same-operation');
      expect(fake.acknowledgements.single.revision, BigInt.from(8));
    },
  );

  test(
    'lost acknowledgement preserves confirmed receipt and frozen intent',
    () async {
      final source = VersionedIdeaView.fromRecord(record());
      final fake = AcknowledgingSession();
      fake.onSave = (_, _) async => result(record(revision: BigInt.from(8)));
      fake.onAcknowledge = () async => throw StateError('ack reply lost');
      final adapter = VersionedWorkbenchEditorAdapter(
        fake,
        source,
        (current) async => presented(current),
      );
      await expectLater(
        adapter.save(draft(source), editor()),
        throwsA(
          isA<VersionedEditorPresentationFailure>().having(
            (failure) => failure.receipt.operation,
            'original op',
            'same-operation',
          ),
        ),
      );
      await expectLater(
        adapter.save(draft(source, title: 'Newer'), editor(title: 'Newer')),
        throwsStateError,
      );
      expect(fake.saves, 1);
      fake.onAcknowledge = null;
      await adapter.save(draft(source), editor());
      expect(fake.acknowledgements.length, 2);
      expect(fake.acknowledgements.map((value) => value.operation).toSet(), {
        'same-operation',
      });
    },
  );

  test(
    'stage completes before submit, preserves u64 asset order and aliases',
    () async {
      final huge = (BigInt.one << 64) - BigInt.one;
      final existing = VersionedAsset(
        id: 'old-asset',
        name: 'old.bin',
        kind: 'file',
        bytes: huge,
      );
      final source = VersionedIdeaView.fromRecord(record(assets: [existing]));
      final oldAttachment = IdeaAttachment.versioned(
        source: const TextureSource(
          location: 'preview/old.bin',
          name: 'old.bin',
          kind: TextureKind.file,
          local: true,
        ),
        byteLength: huge,
        pluginId: existing.id,
      );
      final newAttachment = IdeaAttachment(
        source: const TextureSource(
          location: 'C:/selected/new file.bin',
          name: 'new file.bin',
          kind: TextureKind.file,
          local: true,
        ),
        size: 4,
      );
      final ready = Completer<VersionedAsset>();
      final fake = FakeSession();
      fake.onStage = (_) => ready.future;
      fake.onSave = (_, _) async =>
          result(record(revision: BigInt.from(8), assets: [existing]));
      final adapter = VersionedWorkbenchEditorAdapter(
        fake,
        source,
        (current) async => presented(current),
      );
      final raw = 'Text [file](attachment:C%3A%2Fselected%2Fnew%20file.bin)';
      final first = adapter.save(
        draft(
          source,
          attachments: [oldAttachment, newAttachment],
          description: raw,
        ),
        editor(description: raw),
      );
      await Future<void>.delayed(Duration.zero);
      expect(fake.stages, 1);
      expect(fake.saves, 0);
      ready.complete(
        VersionedAsset(
          id: 'new-asset',
          name: 'new file.bin',
          kind: 'file',
          bytes: BigInt.from(4),
        ),
      );
      await first;
      expect(fake.saves, 1);
      expect(fake.lastFields!.assets.map((a) => a.id), [
        'old-asset',
        'new-asset',
      ]);
      expect(fake.lastFields!.assets.first.bytes, huge);
      expect(fake.lastFields!.description, contains('attachment:new-asset'));
      expect(fake.lastEditor!.description, raw);

      await adapter.save(
        draft(
          source,
          attachments: [oldAttachment, newAttachment],
          description: raw,
        ),
        editor(description: raw),
      );
      expect(fake.stages, 1);
      expect(fake.saves, 2);
    },
  );

  test(
    'rejects V1, task text, changed category, and forged asset before stage',
    () async {
      final source = VersionedIdeaView.fromRecord(
        record(
          assets: [
            VersionedAsset(
              id: 'asset',
              name: 'real.bin',
              kind: 'file',
              bytes: BigInt.from(4),
            ),
          ],
        ),
      );
      final fake = FakeSession();
      final adapter = VersionedWorkbenchEditorAdapter(
        fake,
        source,
        (current) async => presented(current),
      );
      await expectLater(
        adapter.save(draft(source, todos: ['not a TaskId']), editor()),
        throwsA(isA<EditorPreparationException>()),
      );
      await expectLater(
        adapter.save(draft(source, category: '实验'), editor()),
        throwsA(isA<EditorPreparationException>()),
      );
      final forged = IdeaAttachment.versioned(
        source: const TextureSource(
          location: 'preview',
          name: 'wrong.bin',
          kind: TextureKind.file,
        ),
        byteLength: BigInt.from(4),
        pluginId: 'asset',
      );
      await expectLater(
        adapter.save(draft(source, attachments: [forged]), editor()),
        throwsA(isA<EditorPreparationException>()),
      );
      final noVersion = Idea(
        'Edited',
        'Edited body',
        source.category,
        Idea.icons[source.iconIndex],
        Color(source.colorArgb),
        id: source.id,
        stage: source.stage,
        favorite: source.favorite,
      );
      await expectLater(
        adapter.save(noVersion, editor()),
        throwsA(isA<EditorPreparationException>()),
      );
      expect(fake.stages, 0);
      expect(fake.saves, 0);
    },
  );

  test(
    'presentation may advance beyond the historical commit revision',
    () async {
      final source = VersionedIdeaView.fromRecord(record());
      final fake = FakeSession();
      fake.onSave = (_, _) async => result(record(revision: BigInt.from(8)));
      final adapter = VersionedWorkbenchEditorAdapter(
        fake,
        source,
        (_) async => presented(record(revision: BigInt.from(9))),
      );
      final view = await adapter.save(draft(source), editor());
      expect(view.contentRevision, BigInt.from(9));
      expect(view.versioned!.revision, BigInt.from(9));
      expect(fake.saves, 1);
    },
  );
  test(
    'unknown save and presentation failure retain the original stage and intent',
    () async {
      final source = VersionedIdeaView.fromRecord(record());
      final attachment = IdeaAttachment(
        source: const TextureSource(
          location: 'C:/file.bin',
          name: 'file.bin',
          kind: TextureKind.file,
          local: true,
        ),
        size: 4,
      );
      final fake = FakeSession()
        ..onStage = (_) async => VersionedAsset(
          id: 'staged',
          name: 'file.bin',
          kind: 'file',
          bytes: BigInt.from(4),
        );
      var failSave = true, failPresentation = true;
      fake.onSave = (_, _) async {
        if (failSave) {
          failSave = false;
          throw StateError('unknown transport outcome');
        }
        return result(record(revision: BigInt.from(8)));
      };
      final adapter = VersionedWorkbenchEditorAdapter(fake, source, (
        current,
      ) async {
        if (failPresentation) {
          failPresentation = false;
          throw StateError('export unavailable');
        }
        return presented(current);
      });
      final original = draft(source, attachments: [attachment]);
      await expectLater(adapter.save(original, editor()), throwsStateError);
      expect(fake.stages, 1);
      await expectLater(
        adapter.save(
          draft(source, attachments: [attachment], title: 'Changed'),
          editor(),
        ),
        throwsStateError,
      );
      await expectLater(
        adapter.save(original, editor()),
        throwsA(isA<VersionedEditorPresentationFailure>()),
      );
      expect(fake.stages, 1);
      await adapter.save(original, editor());
      expect(fake.stages, 1);
      expect(fake.saves, 3);

      expect(adapter.targetId, source.id);
      expect(adapter.studio, same(fake.fakeStudio));
      await adapter.recordPaste(
        const PasteInsertion(
          id: 'paste',
          field: 'description',
          before: '',
          startUtf16: 0,
          endUtf16: 0,
          parts: [PastePart.literal('text')],
          after: 'text',
        ),
      );
      await adapter.close();
      expect(fake.pastes, 1);
      expect(fake.closes, 1);
      await expectLater(adapter.save(original, editor()), throwsStateError);
    },
  );
}
