import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/studio_backend.dart';

class _Studio implements StudioBackend {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _SaveCall {
  _SaveCall(this.draft, this.fields, this.done);
  final Idea draft;
  final EditorFields fields;
  final Completer<Idea> done;
}

class _Editor implements WorkbenchEditorSession, WorkbenchEditorContinuation {
  _Editor(this.targetId);
  @override
  final String targetId;
  @override
  final StudioBackend studio = _Studio();
  final calls = <_SaveCall>[];
  final confirmed = <Idea>[];
  final continuations = <Completer<WorkbenchEditorSession>>[];
  int closes = 0;

  @override
  Future<void> recordPaste(PasteInsertion _) async {}

  @override
  Future<Idea> save(Idea draft, EditorFields fields) {
    final done = Completer<Idea>();
    calls.add(_SaveCall(draft, fields, done));
    return done.future;
  }

  @override
  Future<WorkbenchEditorSession> continueAfterCommit(Idea result) {
    confirmed.add(result);
    final done = Completer<WorkbenchEditorSession>();
    continuations.add(done);
    return done.future;
  }

  @override
  Future<void> close() async {
    closes++;
  }
}

Idea _initial({List<IdeaAttachment> attachments = const []}) => Idea(
  'Original',
  'Original body',
  '灵感',
  Idea.icons[0],
  const Color(0xff8877aa),
  id: 'editor-draft-coexistence',
  attachments: attachments,
);

Future<void> _open(
  WidgetTester tester,
  _Editor editor,
  void Function(Idea?) returned, {
  Idea? initial,
  Locale? locale,
  bool Function()? isCurrent,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      locale: locale,
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      home: Builder(
        builder: (context) => Scaffold(
          body: TextButton(
            onPressed: () => showDialog<Idea>(
              context: context,
              builder: (_) => NewIdeaDialog(
                initialIdea: initial ?? _initial(),
                editor: editor,
                isCurrent: isCurrent,
              ),
            ).then(returned),
            child: const Text('open editor'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open editor'));
  await tester.pumpAndSettle();
}

final _title = find.byKey(const ValueKey('idea-title'));
final _description = find.byKey(const ValueKey('idea-description'));
final _saveButton = find.byKey(const ValueKey('idea-save'));

Future<void> _save(WidgetTester tester) async {
  await tester.ensureVisible(_saveButton);
  await tester.tap(_saveButton);
  await tester.pump();
}

TextEditingController _controller(WidgetTester tester, Finder field) =>
    tester.widget<TextField>(field).controller!;

void main() {
  testWidgets('unchanged S1 success closes normally without opening S2', (
    tester,
  ) async {
    final first = _Editor('editor-draft-coexistence');
    Idea? returned;
    await _open(tester, first, (value) => returned = value);
    await tester.enterText(_title, 'S1 title');
    await tester.enterText(_description, 'S1 body');
    await _save(tester);
    expect(first.calls, hasLength(1));
    expect(first.calls.single.fields.title, 'S1 title');
    first.calls.single.done.complete(first.calls.single.draft);
    await tester.pumpAndSettle();
    expect(find.byType(NewIdeaDialog), findsNothing);
    expect(returned?.title, 'S1 title');
    expect(first.continuations, isEmpty);
    expect(tester.takeException(), isNull);
  });

  for (final directionOnly in [false, true]) {
    testWidgets(
      'selection-only change during S1 keeps the editor ($directionOnly)',
      (tester) async {
        final first = _Editor('editor-draft-coexistence');
        final second = _Editor('editor-draft-coexistence');
        Idea? returned;
        await _open(tester, first, (value) => returned = value);
        await tester.enterText(_title, 'S1 title');
        await tester.enterText(_description, 'S1 body');
        await _save(tester);
        final controller = _controller(tester, _description);
        final previous = controller.value;
        final selection = directionOnly
            ? TextSelection(
                baseOffset: previous.selection.baseOffset,
                extentOffset: previous.selection.extentOffset,
                affinity: TextAffinity.upstream,
                isDirectional: true,
              )
            : const TextSelection(baseOffset: 1, extentOffset: 4);
        final live = previous.copyWith(selection: selection);
        expect(live.text, previous.text);
        expect(live.composing, previous.composing);
        expect(live, isNot(previous));
        controller.value = live;
        await tester.pump();
        first.calls.single.done.complete(first.calls.single.draft);
        await tester.pump();
        expect(first.continuations, hasLength(1));
        first.continuations.single.complete(second);
        await tester.pumpAndSettle();
        expect(find.byType(NewIdeaDialog), findsOneWidget);
        expect(returned, isNull);
        expect(controller.value, live);
        expect(second.calls, isEmpty);
        await tester.pumpWidget(const SizedBox());
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'delayed S1 keeps text editable and preserves S2 selection and composition',
    (tester) async {
      final first = _Editor('editor-draft-coexistence');
      final second = _Editor('editor-draft-coexistence');
      Idea? returned;
      await _open(tester, first, (value) => returned = value);
      await tester.enterText(_title, 'S1 title');
      await tester.enterText(_description, 'S1 body');
      await _save(tester);
      expect(first.calls, hasLength(1));
      expect(tester.widget<TextField>(_title).readOnly, isFalse);
      expect(tester.widget<TextField>(_description).readOnly, isFalse);
      await tester.enterText(_title, 'S2 title');
      await tester.enterText(_description, 'S2 body');
      final description = _controller(tester, _description);
      const liveValue = TextEditingValue(
        text: 'S2 body',
        selection: TextSelection(baseOffset: 2, extentOffset: 5),
        composing: TextRange(start: 2, end: 5),
      );
      description.value = liveValue;
      await tester.pump();
      expect(first.calls.single.draft.title, 'S1 title');
      expect(first.calls.single.fields.description, 'S1 body');

      first.calls.single.done.complete(first.calls.single.draft);
      await tester.pump();
      expect(first.confirmed, [same(first.calls.single.draft)]);
      expect(first.continuations, hasLength(1));
      first.continuations.single.complete(second);
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(returned, isNull);
      expect(_controller(tester, _title).text, 'S2 title');
      expect(_controller(tester, _description).value, liveValue);
      expect(first.calls, hasLength(1));
      expect(second.calls, isEmpty);

      await _save(tester);
      expect(second.calls, hasLength(1));
      expect(second.calls.single.draft.title, 'S2 title');
      expect(second.calls.single.fields.description, 'S2 body');
      second.calls.single.done.complete(second.calls.single.draft);
      await tester.pumpAndSettle();
      expect(returned?.title, 'S2 title');
      expect(find.byType(NewIdeaDialog), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Unknown outcome retries exact S1 despite empty newer title, then waits for explicit S2',
    (tester) async {
      final first = _Editor('editor-draft-coexistence');
      final second = _Editor('editor-draft-coexistence');
      await _open(tester, first, (_) {});
      await tester.enterText(_title, 'S1 title');
      await tester.enterText(_description, 'S1 body');
      await _save(tester);
      await tester.enterText(_title, '');
      await tester.enterText(_description, 'S2 body with empty title');
      first.calls.single.done.completeError(StateError('Unknown outcome'));
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(find.byKey(const ValueKey('idea-save-error')), findsOneWidget);
      expect(tester.widget<TextField>(_title).readOnly, isFalse);
      expect(_controller(tester, _title).text, isEmpty);
      await _save(tester);
      expect(first.calls, hasLength(2));
      expect(identical(first.calls[0].draft, first.calls[1].draft), isTrue);
      expect(identical(first.calls[0].fields, first.calls[1].fields), isTrue);
      expect(first.calls[1].draft.title, 'S1 title');
      expect(first.calls[1].fields.description, 'S1 body');
      first.calls[1].done.complete(first.calls[1].draft);
      await tester.pump();
      expect(first.continuations, hasLength(1));
      first.continuations.single.complete(second);
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(_controller(tester, _title).text, isEmpty);
      expect(
        _controller(tester, _description).text,
        'S2 body with empty title',
      );
      expect(second.calls, isEmpty);
      await _save(tester);
      expect(second.calls, isEmpty);
      await tester.enterText(_title, 'S2 title');
      await _save(tester);
      expect(second.calls, hasLength(1));
      expect(second.calls.single.draft.title, 'S2 title');
      second.calls.single.done.complete(second.calls.single.draft);
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'failed successor open retries continuation without resubmitting confirmed S1',
    (tester) async {
      final first = _Editor('editor-draft-coexistence');
      final second = _Editor('editor-draft-coexistence');
      await _open(tester, first, (_) {});
      await tester.enterText(_title, 'S1 title');
      await tester.enterText(_description, 'S1 body');
      await _save(tester);
      await tester.enterText(_title, 'S2 title');
      await tester.enterText(_description, 'S2 body');
      first.calls.single.done.complete(first.calls.single.draft);
      await tester.pump();
      expect(first.continuations, hasLength(1));
      first.continuations.single.completeError(
        StateError('successor unavailable'),
      );
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(find.byKey(const ValueKey('idea-save-error')), findsOneWidget);
      expect(first.calls, hasLength(1));
      expect(_controller(tester, _title).text, 'S2 title');
      expect(second.calls, isEmpty);

      await _save(tester);
      expect(first.calls, hasLength(1));
      expect(first.continuations, hasLength(2));
      expect(first.confirmed[0], same(first.confirmed[1]));
      first.continuations[1].complete(second);
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(second.calls, isEmpty);
      await _save(tester);
      expect(second.calls, hasLength(1));
      expect(second.calls.single.draft.title, 'S2 title');
      second.calls.single.done.complete(second.calls.single.draft);
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'late alias error retains opened S2 for correction and explicit save',
    (tester) async {
      IdeaAttachment source(String folder) => IdeaAttachment(
        source: TextureSource(
          location: 'C:/$folder/shared.bin',
          name: 'shared.bin',
          kind: TextureKind.file,
          local: true,
        ),
        size: 4,
      );
      IdeaAttachment committed(String id) => IdeaAttachment.versioned(
        source: TextureSource(
          location: 'C:/preview/$id.bin',
          name: 'shared.bin',
          kind: TextureKind.file,
          local: true,
        ),
        byteLength: BigInt.from(4),
        pluginId: id,
      );
      final firstFile = source('first');
      final secondFile = source('second');
      final first = _Editor('editor-draft-coexistence');
      final second = _Editor('editor-draft-coexistence');
      Idea? returned;
      await _open(
        tester,
        first,
        (value) => returned = value,
        initial: _initial(attachments: [firstFile, secondFile]),
      );
      await tester.enterText(_title, 'S1 title');
      await tester.enterText(_description, 'S1 body');
      await _save(tester);
      await tester.enterText(_title, 'S2 title');
      await tester.enterText(_description, 'attachment:shared.bin');
      final submitted = first.calls.single.draft;
      final confirmed = Idea(
        submitted.title,
        submitted.description,
        submitted.category,
        submitted.icon,
        submitted.color,
        id: submitted.id,
        stage: submitted.stage,
        attachments: [committed('asset-first'), committed('asset-second')],
      );
      first.calls.single.done.complete(confirmed);
      await tester.pump();
      expect(first.continuations, hasLength(1));
      first.continuations.single.complete(second);
      await tester.pumpAndSettle();

      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(find.byKey(const ValueKey('idea-save-error')), findsOneWidget);
      expect(_controller(tester, _title).text, 'S2 title');
      expect(_controller(tester, _description).text, 'attachment:shared.bin');
      expect(first.calls, hasLength(1));
      expect(first.continuations, hasLength(1));
      expect(second.calls, isEmpty);
      expect(second.closes, 0);
      expect(returned, isNull);

      await tester.enterText(
        _description,
        'attachment:C:/first/shared.bin and attachment:C:/second/shared.bin',
      );
      await _save(tester);
      await tester.pumpAndSettle();
      expect(first.calls, hasLength(1));
      expect(first.continuations, hasLength(1));
      expect(second.calls, isEmpty);
      expect(second.closes, 0);
      expect(
        _controller(tester, _description).text,
        'attachment:asset-first and attachment:asset-second',
      );

      await _save(tester);
      expect(second.calls, hasLength(1));
      expect(second.calls.single.draft.title, 'S2 title');
      expect(
        second.calls.single.fields.description,
        'attachment:asset-first and attachment:asset-second',
      );
      second.calls.single.done.complete(second.calls.single.draft);
      await tester.pumpAndSettle();
      expect(returned?.title, 'S2 title');
      expect(find.byType(NewIdeaDialog), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'pending, continuation failure, and newer draft fit nine locales',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(360, 640);
      addTearDown(tester.view.reset);
      for (final code in [
        'zh',
        'en',
        'ru',
        'fr',
        'de',
        'es',
        'ja',
        'ko',
        'pt',
      ]) {
        await tester.pumpWidget(const SizedBox());
        final first = _Editor('editor-draft-coexistence');
        final second = _Editor('editor-draft-coexistence');
        await _open(tester, first, (_) {}, locale: Locale(code));
        expect(tester.takeException(), isNull, reason: 'initial: $code');
        await tester.enterText(_title, 'S1 title');
        await _save(tester);
        expect(find.byKey(const ValueKey('idea-draft-status')), findsOneWidget);
        expect(tester.takeException(), isNull, reason: 'pending: $code');

        await tester.enterText(_title, 'S2 title');
        first.calls.single.done.complete(first.calls.single.draft);
        await tester.pump();
        first.continuations.single.completeError(StateError('open failed'));
        await tester.pumpAndSettle();
        expect(find.byKey(const ValueKey('idea-save-error')), findsOneWidget);
        expect(find.byKey(const ValueKey('idea-draft-status')), findsOneWidget);
        expect(
          tester.takeException(),
          isNull,
          reason: 'continue failure: $code',
        );

        await _save(tester);
        expect(first.continuations, hasLength(2));
        first.continuations.last.complete(second);
        await tester.pumpAndSettle();
        expect(find.byKey(const ValueKey('idea-draft-status')), findsOneWidget);
        expect(_controller(tester, _title).text, 'S2 title');
        expect(tester.takeException(), isNull, reason: 'newer draft: $code');
      }
    },
  );

  testWidgets(
    'workspace change while successor opens closes it and keeps typed S2',
    (tester) async {
      var current = true;
      final first = _Editor('editor-draft-coexistence');
      final second = _Editor('editor-draft-coexistence');
      Idea? returned;
      await _open(
        tester,
        first,
        (value) => returned = value,
        isCurrent: () => current,
      );
      await tester.enterText(_title, 'S1 title');
      await _save(tester);
      await tester.enterText(_title, 'Typed S2 title');
      await tester.enterText(_description, 'Typed S2 body');
      first.calls.single.done.complete(first.calls.single.draft);
      await tester.pump();
      expect(first.continuations, hasLength(1));

      current = false;
      first.continuations.single.complete(second);
      await tester.pumpAndSettle();
      expect(second.closes, 1);
      expect(second.calls, isEmpty);
      expect(first.calls, hasLength(1));
      expect(_controller(tester, _title).text, 'Typed S2 title');
      expect(_controller(tester, _description).text, 'Typed S2 body');
      expect(find.byKey(const ValueKey('idea-save-error')), findsOneWidget);
      expect(returned, isNull);
      expect(tester.takeException(), isNull);
    },
  );
}
