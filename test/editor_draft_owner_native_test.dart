import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart' show NewIdeaDialog, Idea;
import 'package:morrow_studio/card_tips_editor.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_commit_proof.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available = Platform.isWindows && _host != null && _package != null;

final class _LostBusinessReply
    implements
        WorkbenchEditorSession,
        WorkbenchEditorOperationIdentity,
        WorkbenchEditorDraftCaptureIdentity,
        WorkbenchEditorCommitSource {
  _LostBusinessReply(this.original);
  final WorkbenchEditorSession original;
  int saves = 0;
  @override
  String get targetId => original.targetId;
  @override
  StudioBackend get studio => original.studio;
  @override
  String get commitOperation =>
      (original as WorkbenchEditorOperationIdentity).commitOperation;
  @override
  BigInt get sourceRevision =>
      (original as WorkbenchEditorOperationIdentity).sourceRevision;
  @override
  ({String scope, int events, bool complete}) get draftCapture =>
      (original as WorkbenchEditorDraftCaptureIdentity).draftCapture;
  @override
  Future<void> recordPaste(PasteInsertion insertion) =>
      original.recordPaste(insertion);
  @override
  Future<void> close() => original.close();
  @override
  Future<EditorDraftCommitEvidence> inspectCommittedSource() =>
      (original as WorkbenchEditorCommitSource).inspectCommittedSource();
  @override
  Future<Idea> save(Idea draft, EditorFields fields) async {
    saves++;
    await original.save(draft, fields);
    throw StateError('test withheld the original business receipt');
  }
}

final class _LostProposalReply implements EditorDraftHandoffProposalControl {
  _LostProposalReply(this.original, this.lostStage);
  final EditorDraftHandoffProposalControl original;
  final String lostStage;
  final calls = <String, int>{};
  void Function()? afterComplete;
  Future<EditorDraftHandoffProposalRecord> _dispatch(
    String stage,
    Future<EditorDraftHandoffProposalRecord> Function() work,
  ) async {
    calls[stage] = (calls[stage] ?? 0) + 1;
    final result = await work();
    if (stage == 'complete') afterComplete?.call();
    if (stage == lostStage) {
      throw StateError('test withheld original $stage receipt');
    }
    return result;
  }

  @override
  Future<EditorDraftHandoffProposalRecord> prepare(
    EditorDraftHandoffProposal proposal,
  ) => _dispatch('prepare', () => original.prepare(proposal));
  @override
  Future<EditorDraftHandoffProposalRecord> complete({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _dispatch(
    'complete',
    () => original.complete(
      cardId: cardId,
      parentDraftId: parentDraftId,
      childOperation: childOperation,
    ),
  );
  @override
  Future<EditorDraftHandoffProposalRecord> retire({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => _dispatch(
    'retire',
    () => original.retire(
      cardId: cardId,
      parentDraftId: parentDraftId,
      childOperation: childOperation,
    ),
  );
  @override
  Future<EditorDraftHandoffProposalRecord?> inspect({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => original.inspect(
    cardId: cardId,
    parentDraftId: parentDraftId,
    childOperation: childOperation,
  );
  @override
  Future<EditorDraftHandoffProposalRecord> cancel({
    required String cardId,
    required String parentDraftId,
    required String childOperation,
  }) => original.cancel(
    cardId: cardId,
    parentDraftId: parentDraftId,
    childOperation: childOperation,
  );
  @override
  Future<EditorDraftHandoffProposalPage> page({
    String cursor = '',
    int limit = 32,
  }) => original.page(cursor: cursor, limit: limit);
  @override
  Future<List<EditorDraftHandoffProposalSummary>> discover({
    int pageSize = 32,
  }) => original.discover(pageSize: pageSize);
}

final class _LostChildReply implements EditorDraftControl {
  _LostChildReply(this.original);
  final EditorDraftControl original;
  final operations = <String>[];
  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) async {
    operations.add(request.operation);
    final record = await original.save(request);
    if (operations.length == 1) {
      throw StateError('test withheld original child update receipt');
    }
    return record;
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) =>
      original.read(cardId, draftId);
  @override
  Future<List<EditorDraftSummary>> list() => original.list();
  @override
  Future<EditorDraftRecord> discard(String c, String d, BigInt g, String o) =>
      original.discard(c, d, g, o);
  @override
  Future<EditorDraftImportedAsset> importAsset(
    String c,
    String d,
    BigInt g,
    String p,
    String n,
    String k,
    BigInt b,
  ) => original.importAsset(c, d, g, p, n, k, b);
  @override
  Future<void> exportAsset(String c, String d, BigInt g, String a, String p) =>
      original.exportAsset(c, d, g, a, p);
}

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftSnapshot _snapshot() => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _text(''),
    description: _text(''),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: '实验',
    stage: '待验证',
  ),
  assets: const [],
);

Future<T> _hostFuture<T>(WidgetTester tester, Future<T> request) async {
  var done = false;
  late T result;
  Object? failure;
  request.then(
    (value) {
      result = value;
      done = true;
    },
    onError: (Object error) {
      failure = error;
      done = true;
    },
  );
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!done) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Original host request did not settle');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump();
  }
  if (failure != null) throw failure!;
  return result;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  for (final lost in ['business', 'prepare', 'complete', 'child', 'retire']) {
    test(
      'root recovers lost $lost receipt by original inspection without replay',
      () async {
        final root = await Directory.systemTemp.createTemp(
          'morrow-lost-$lost-',
        );
        RustWorkbench? host;
        try {
          host = await RustWorkbench.open(
            executable: _host!,
            package: _package!,
            directory: Directory('${root.path}/store'),
          );
          final native = await host.openEditor('lost-$lost', create: true);
          final editor = lost == 'business'
              ? _LostBusinessReply(native)
              : native;
          final initial = _snapshot().values;
          final owner = await host.createEditorDraftOwner(
            editor: editor,
            initial: EditorDraftSnapshot(
              values: EditorDraftValues(
                title: _text('S1'),
                description: _text('S1 body'),
                hypothesis: initial.hypothesis,
                conclusion: initial.conclusion,
                todos: initial.todos,
                category: initial.category,
                stage: initial.stage,
              ),
              assets: const [],
            ),
            attachments: const [],
            create: true,
          );
          final base = host.editorDraftCommitServices(owner);
          final proposals = _LostProposalReply(base.proposals, lost);
          final lostChild = _LostChildReply(base.drafts);
          void input(EditorDraftOwner target, String body) {
            final raw = target.session.current;
            target.session.observe(
              EditorDraftSnapshot(
                values: EditorDraftValues(
                  title: raw.values.title,
                  description: _text(body),
                  hypothesis: raw.values.hypothesis,
                  conclusion: raw.values.conclusion,
                  todos: raw.values.todos,
                  category: raw.values.category,
                  stage: raw.values.stage,
                ),
                assets: raw.assets,
                captureScope: raw.captureScope,
                captureEvents: raw.captureEvents,
              ),
            );
          }

          if (lost == 'child') {
            proposals.afterComplete = () =>
                input(owner, 'S2 during first child');
          }
          var acknowledgements = 0;
          final services = EditorDraftCommitServices(
            drafts: lost == 'child' ? lostChild : base.drafts,
            proposals: proposals,
            inspectCommit: base.inspectCommit,
            readSource: base.readSource,
            presentSource: base.presentSource,
            openSuccessor: base.openSuccessor,
            retain: base.retain,
            acknowledge: (editor, proof) async {
              acknowledgements++;
              await base.acknowledge(editor, proof);
            },
          );
          final attempt = owner.beginCommit(
            Idea(
              'S1',
              'S1 body',
              '实验',
              Icons.science,
              const Color(0xffaabbcc),
              id: 'lost-$lost',
            ),
            services,
          );
          if (lost == 'retire') {
            await attempt.start();
            attempt.transferView((_) {});
            await expectLater(attempt.finish(), throwsStateError);
          } else {
            await expectLater(attempt.start(), throwsA(anything));
          }
          expect(attempt.stage, lost);
          expect(attempt.failure, isNotNull);
          expect(acknowledgements, 0);
          await expectLater(host.prepareEditorDraftClose(), throwsStateError);
          input(
            lost == 'retire' ? attempt.child! : owner,
            'S2 after lost $lost',
          );
          final child = await attempt.inspectAndContinue();
          if (!attempt.viewTransferred) attempt.transferView((_) {});
          await attempt.finish();
          expect(attempt.finished, isTrue);
          expect(acknowledgements, 1);
          expect(proposals.calls, {'prepare': 1, 'complete': 1, 'retire': 1});
          if (editor is _LostBusinessReply) expect(editor.saves, 1);
          if (lost == 'child') {
            expect(lostChild.operations.length, 2);
            expect(
              lostChild.operations.toSet().length,
              2,
              reason:
                  'The lost original is inspected; newer S2 uses one new operation',
            );
          }
          final persisted = (await host.editorDrafts.read(
            'lost-$lost',
            child.session.draftId,
          ))!;
          expect(
            persisted.request.values.description.text,
            'S2 after lost $lost',
          );
          expect(
            (await host.versionedContent.read('lost-$lost')).revision,
            BigInt.one,
          );
          final preparation = await host.prepareEditorDraftClose();
          await host.finishEditorDraftClose(preparation);
          await host.close();
          host = null;
        } finally {
          if (host == null) await root.delete(recursive: true);
        }
      },
      skip: !_available,
    );
  }

  testWidgets(
    'formal save transfers the live view and preserves input during S1',
    (tester) async {
      late Directory root;
      late RustWorkbench host;
      late EditorDraftOwner owner;
      var closed = false;
      await tester.runAsync(() async {
        root = await Directory.systemTemp.createTemp('morrow-formal-commit-');
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final editor = await host.openEditor('formal-commit', create: true);
        owner = await host.createEditorDraftOwner(
          editor: editor,
          initial: _snapshot(),
          attachments: const [],
          create: true,
        );
      });
      Future<void> until(bool Function() ready) async {
        final deadline = DateTime.now().add(const Duration(seconds: 35));
        while (!ready()) {
          if (DateTime.now().isAfter(deadline)) {
            fail(
              'Original formal commit stopped at ${owner.commit?.stage}: ${owner.commit?.failure}',
            );
          }
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 10)),
          );
          await tester.pump();
        }
      }

      try {
        await tester.pumpWidget(
          MaterialApp(
            locale: const Locale('zh'),
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            home: Scaffold(
              body: NewIdeaDialog(
                editor: owner.editor,
                draftOwner: owner,
                draftRuntime: host,
                initialCategory: '实验',
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        TextEditingController field(String key) =>
            tester.widget<TextField>(find.byKey(ValueKey(key))).controller!;
        field('idea-title').text = 'Formal S1';
        field('idea-description').text = 'Formal S1 body';
        final save = tester.widget<FilledButton>(
          find.byKey(const ValueKey('idea-save')),
        );
        save.onPressed!();
        await until(
          () =>
              owner.commit?.stage == 'business' ||
              owner.commit?.failure != null,
        );
        expect(owner.commit!.failure, isNull);
        field('idea-description').value = const TextEditingValue(
          text: 'Formal S2 😀',
          selection: TextSelection(
            baseOffset: 10,
            extentOffset: 12,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: TextRange(start: 10, end: 12),
        );
        await until(
          () => owner.commit!.finished || owner.commit!.failure != null,
        );
        expect(owner.commit!.failure, isNull);
        expect(owner.commit!.finished, isTrue);
        final child = owner.commit!.child!;
        expect(host.editorDraftWorkspace.attachedViews, 1);
        expect(host.editorDraftWorkspace.hasView(child.session), isTrue);
        expect(child.session.current.values.description.text, 'Formal S2 😀');
        expect(child.session.current.values.description.composingEnd, 12);
        expect(find.byKey(const ValueKey('idea-description')), findsOneWidget);
        expect(find.byKey(const ValueKey('idea-save-error')), findsNothing);
        await tester.pumpWidget(const SizedBox.shrink());
        final preparation = await _hostFuture(
          tester,
          host.prepareEditorDraftClose(),
        );
        await _hostFuture(tester, host.finishEditorDraftClose(preparation));
        await _hostFuture(tester, host.close());
        closed = true;
        await tester.runAsync(() async {
          host = await RustWorkbench.open(
            executable: _host!,
            package: _package!,
            directory: Directory('${root.path}/store'),
          );
          closed = false;
          final restored = (await host.editorDrafts.read(
            'formal-commit',
            child.session.draftId,
          ))!;
          expect(restored.request.values.description.text, 'Formal S2 😀');
          expect(restored.request.values.description.selectionBase, 10);
          expect(
            (await host.versionedContent.read('formal-commit')).revision,
            BigInt.one,
          );
          await host.close();
          closed = true;
        });
        expect(tester.takeException(), isNull);
      } finally {
        if (closed) await tester.runAsync(() => root.delete(recursive: true));
      }
    },
    skip: !_available,
  );

  test(
    'root commit keeps complete S2 before parent retirement and survives host restart',
    () async {
      final root = await Directory.systemTemp.createTemp(
        'morrow-owned-commit-',
      );
      RustWorkbench? host;
      try {
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final editor = await host.openEditor('owned-commit', create: true);
        final initial = _snapshot().values;
        final owner = await host.createEditorDraftOwner(
          editor: editor,
          initial: EditorDraftSnapshot(
            values: EditorDraftValues(
              title: _text('S1 title'),
              description: _text('S1 body'),
              hypothesis: initial.hypothesis,
              conclusion: initial.conclusion,
              todos: initial.todos,
              category: initial.category,
              stage: initial.stage,
            ),
            assets: const [],
          ),
          attachments: const [],
          create: true,
        );
        final template = Idea(
          'S1 title',
          'S1 body',
          '实验',
          Icons.science,
          const Color(0xffaabbcc),
          id: 'owned-commit',
        );
        final attempt = host.beginEditorDraftCommit(owner, template);
        final child = await attempt.start();
        expect(attempt.confirmed!.contentRevision, BigInt.one);
        expect(
          attempt.frozenParent!.request.commitOperation,
          (editor as WorkbenchEditorOperationIdentity).commitOperation,
        );
        expect(
          attempt.proposalRecord!.summary.status,
          EditorDraftHandoffProposalStatus.childCommitted,
        );
        expect(
          (await host.editorDrafts.read(
            'owned-commit',
            owner.session.draftId,
          ))!.active,
          isTrue,
        );
        final current = owner.session.current;
        owner.session.observe(
          EditorDraftSnapshot(
            values: EditorDraftValues(
              title: current.values.title,
              description: const EditorDraftTextValue(
                text: 'S2 😀 raw',
                selectionBase: 3,
                selectionExtent: 5,
                affinity: 0,
                directional: true,
                composingStart: 3,
                composingEnd: 5,
              ),
              hypothesis: _text('S2 hypothesis'),
              conclusion: _text('S2 conclusion'),
              todos: _text('one\n\ntwo'),
              category: '进行中',
              stage: '推进中',
            ),
            assets: current.assets,
            captureScope: current.captureScope,
            captureEvents: current.captureEvents,
          ),
        );
        attempt.transferView((_) {});
        expect(attempt.hasSuccessorChanges, isTrue);
        await attempt.finish();
        expect(attempt.finished, isTrue);
        expect(owner.retired, isTrue);
        expect(owner.captureClosed, isTrue);
        expect(owner.session.disposed, isTrue);
        expect(host.editorDraftWorkspace.sessions, [child.session]);
        expect(child.session.dirty, isFalse);
        final childId = child.session.draftId;
        final parentId = owner.session.draftId;
        final preparation = await host.prepareEditorDraftClose();
        await host.finishEditorDraftClose(preparation);
        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final restored = (await host.editorDrafts.read(
          'owned-commit',
          childId,
        ))!;
        expect(restored.request.values.description.text, 'S2 😀 raw');
        expect(restored.request.values.description.selectionExtent, 5);
        expect(restored.request.values.description.composingStart, 3);
        expect(restored.request.values.category, '进行中');
        expect(restored.request.values.todos.text, 'one\n\ntwo');
        expect(restored.parentLink!.parentDraftId, parentId);
        expect(
          (await host.editorDrafts.read('owned-commit', parentId))!.active,
          isFalse,
        );
        expect(
          (await host.versionedContent.read('owned-commit')).revision,
          BigInt.one,
        );
        await host.close();
        host = null;
      } finally {
        if (host == null) await root.delete(recursive: true);
      }
    },
    skip: !_available,
  );

  testWidgets(
    'formal NewIdeaDialog raw fields survive view disposal and native host restart',
    (tester) async {
      late Directory root;
      late RustWorkbench host;
      late final EditorDraftOwner owner;
      var closed = false;
      await tester.runAsync(() async {
        root = await Directory.systemTemp.createTemp('morrow-formal-draft-');
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final editor = await host.openEditor('formal-raw-draft', create: true);
        owner = await host.createEditorDraftOwner(
          editor: editor,
          initial: _snapshot(),
          attachments: const [],
          create: true,
        );
      });
      try {
        await tester.pumpWidget(
          MaterialApp(
            locale: const Locale('zh'),
            supportedLocales: AppLocalizations.supportedLocales,
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            home: Scaffold(
              body: NewIdeaDialog(
                editor: owner.editor,
                draftOwner: owner,
                draftRuntime: host,
                initialCategory: '实验',
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        TextEditingController field(String key) =>
            tester.widget<TextField>(find.byKey(ValueKey(key))).controller!;
        field('idea-description').value = const TextEditingValue(
          text: 'formal 😀 raw body',
          selection: TextSelection(
            baseOffset: 7,
            extentOffset: 9,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: TextRange(start: 7, end: 9),
        );
        field('experiment-hypothesis').text = 'raw hypothesis';
        field('experiment-conclusion').text = 'raw conclusion';
        tester
                .widget<CardTipsEditor>(
                  find.byKey(const ValueKey('idea-todos')),
                )
                .controller
                .text =
            'raw task one\n\nraw task two';
        await tester.pump();
        expect(owner.session.current.values.title.text, isEmpty);
        expect(host.editorDraftWorkspace.attachedViews, 1);
        await tester.pumpWidget(const SizedBox.shrink());
        expect(host.editorDraftWorkspace.attachedViews, 0);
        expect(owner.session.disposed, isFalse);
        expect(owner.captureClosed, isFalse);
        final draftId = owner.session.draftId;
        // Pump the widget zone as well as real IO. runAsync alone cannot drain
        // an initial journal future admitted by the dialog's fake-clock zone.
        final preparation = await _hostFuture(
          tester,
          host.prepareEditorDraftClose(),
        );
        await _hostFuture(tester, host.finishEditorDraftClose(preparation));
        await _hostFuture(tester, host.close());
        closed = true;
        await tester.runAsync(() async {
          host = await RustWorkbench.open(
            executable: _host!,
            package: _package!,
            directory: Directory('${root.path}/store'),
          );
          closed = false;
          final record = (await host.editorDrafts.read(
            'formal-raw-draft',
            draftId,
          ))!;
          expect(record.request.values.title.text, isEmpty);
          final body = record.request.values.description;
          expect(body.text, 'formal 😀 raw body');
          expect(body.selectionBase, 7);
          expect(body.selectionExtent, 9);
          expect(body.affinity, TextAffinity.upstream.index);
          expect(body.directional, isTrue);
          expect(body.composingStart, 7);
          expect(body.composingEnd, 9);
          expect(record.request.values.hypothesis.text, 'raw hypothesis');
          expect(record.request.values.conclusion.text, 'raw conclusion');
          expect(
            record.request.values.todos.text,
            'raw task one\n\nraw task two',
          );
          expect(record.request.commitOperation, isNotEmpty);
          await host.close();
          closed = true;
        });
        expect(tester.takeException(), isNull);
      } finally {
        if (closed) await tester.runAsync(() => root.delete(recursive: true));
      }
    },
    skip: !_available,
  );

  test(
    'original workbench retains detached raw draft, import pins and operation through restart',
    () async {
      final root = await Directory.systemTemp.createTemp('morrow-owned-draft-');
      RustWorkbench? host;
      final controllers = List.generate(5, (_) => TextEditingController());
      EditorDraftBinding? binding;
      try {
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final editor = await host.openEditor('owned-new-card', create: true);
        final operation =
            (editor as WorkbenchEditorOperationIdentity).commitOperation;
        final owner = await host.createEditorDraftOwner(
          editor: editor,
          initial: _snapshot(),
          attachments: const [],
          create: true,
        );
        final selected = <IdeaAttachment>[];
        binding = EditorDraftBinding(
          session: owner.session,
          title: controllers[0],
          description: controllers[1],
          hypothesis: controllers[2],
          conclusion: controllers[3],
          todos: controllers[4],
          readMetadata: () =>
              owner.metadata(category: '实验', stage: '待验证', selected: selected),
        );
        final lease = owner.attach(binding);
        controllers[1].value = const TextEditingValue(
          text: 'x😀y 原始正文',
          selection: TextSelection(
            baseOffset: 1,
            extentOffset: 3,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: TextRange(start: 1, end: 3),
        );
        controllers[2].text = '尚未提交的假设';
        controllers[3].text = '尚未提交的结论';
        controllers[4].text = 'one\n\ntwo';
        final captured = await editor.studio.capture(
          'html',
          '<strong>captured</strong>',
        );
        expect(captured.ticket, isNotEmpty);
        final richBody = '${controllers[1].text}\n${captured.markdown}';
        final beforePaste = controllers[1].value;
        await owner.recordPaste(
          PasteInsertion(
            id: 'original-rich-paste',
            field: 'description',
            before: controllers[1].text,
            startUtf16: controllers[1].text.length,
            endUtf16: controllers[1].text.length,
            parts: [
              const PastePart.literal('\n'),
              PastePart.ticket(captured.ticket!),
            ],
            after: richBody,
          ),
        );
        controllers[1].value = beforePaste.copyWith(text: richBody);
        final input = File('${root.path}/selected.txt');
        const bytes = [9, 8, 7, 0, 255, 42];
        await input.writeAsBytes(bytes);
        selected.add(
          await host.importEditorDraftFile(
            owner,
            path: input.path,
            name: 'selected.txt',
            kind: 'file',
          ),
        );
        expect(binding.capture(), isTrue);
        await input.delete();
        await expectLater(host.close(), throwsStateError);
        final preparation = await host.prepareEditorDraftClose();
        final captureFile = File('${root.path}/capture-before.bin');
        await (host.editorDrafts as EditorDraftCaptureExport).exportCapture(
          owner.session.cardId,
          owner.session.draftId,
          owner.session.confirmed!.generation,
          captureFile.path,
        );
        final archivedCapture = await captureFile.readAsBytes();
        expect(archivedCapture.length, greaterThan(1024));
        expect(owner.session.confirmed!.request.commitOperation, operation);
        expect(owner.session.confirmed!.request.values.title.text, isEmpty);
        await expectLater(
          host.finishEditorDraftClose(preparation),
          throwsStateError,
        );
        expect(owner.workspace.disposed, isFalse);
        binding.dispose();
        binding = null;
        lease.release();
        expect(owner.workspace.sessions, contains(owner.session));
        final draftId = owner.session.draftId;
        await host.finishEditorDraftClose(preparation);
        expect(owner.captureClosed, isTrue);
        await host.close();
        host = null;
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final scopes = await host.editorDrafts.list();
        expect(scopes.single.draftId, draftId);
        final record = (await host.editorDrafts.read(
          'owned-new-card',
          draftId,
        ))!;
        expect(record.request.commitOperation, operation);
        expect(record.request.values.description.text, richBody);
        expect(record.request.captureScope, isNotEmpty);
        expect(record.request.captureEvents, 1);
        expect(record.request.values.description.selectionExtent, 3);
        expect(
          record.request.values.description.affinity,
          TextAffinity.upstream.index,
        );
        expect(record.request.values.description.directional, isTrue);
        expect(record.request.values.description.composingStart, 1);
        expect(record.request.values.description.composingEnd, 3);
        expect(record.request.values.hypothesis.text, '尚未提交的假设');
        expect(record.request.values.conclusion.text, '尚未提交的结论');
        expect(record.request.values.todos.text, 'one\n\ntwo');
        final exported = File('${root.path}/restored.txt');
        await host.editorDrafts.exportAsset(
          'owned-new-card',
          draftId,
          record.generation,
          record.assets.single.selection.assetId,
          exported.path,
        );
        expect(await exported.readAsBytes(), bytes);
        final restoredCapture = File('${root.path}/capture-after.bin');
        await (host.editorDrafts as EditorDraftCaptureExport).exportCapture(
          'owned-new-card',
          draftId,
          record.generation,
          restoredCapture.path,
        );
        expect(await restoredCapture.readAsBytes(), archivedCapture);
        await expectLater(
          host.versionedContent.read('owned-new-card'),
          throwsA(anything),
        );
        await host.close();
        host = null;
      } finally {
        binding?.dispose();
        for (final controller in controllers) {
          controller.dispose();
        }
        // A failed owner-close test retains its host and evidence; no forced kill.
        if (host == null) await root.delete(recursive: true);
      }
    },
    skip: !_available,
  );
}
