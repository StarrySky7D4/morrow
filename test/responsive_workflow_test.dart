import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/card_order_preferences.dart';
import 'package:morrow_studio/card_tips_editor.dart';
import 'package:morrow_studio/tip_preferences.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/workspace_viewport.dart';
import 'package:morrow_studio/pending_ui_writes.dart';
import 'package:morrow_studio/attachments/clipboard_import.dart';
import 'package:morrow_studio/plugins/query_coordinator.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';
import 'package:morrow_studio/plugins/application_shutdown.dart';
import 'package:morrow_studio/plugins/workbench_shutdown.dart';
import 'reorder_test.dart' show holdMove;
import 'query_coordinator_test.dart'
    show QueryBackend, conditions, waitForLocalizedWorkbench;
import 'editor_capture_test.dart' show EditorFixture;
import 'tip_settings_test.dart' show click;

class SlowOrderStore extends MemoryTipStore {
  final writeDone = Completer<void>();
  @override
  Future<void> write(String value) async {
    await writeDone.future;
    await super.write(value);
  }
}

class UnchangedEditorBackend extends QueryBackend
    implements WorkbenchEditorSupport {
  UnchangedEditorBackend(this.current);
  final Idea current;
  final editor = EditorFixture();
  @override
  Future<List<Idea>> refreshEditorContent() async => [current];
  @override
  Future<WorkbenchEditorSession> openEditor(
    String target, {
    required bool create,
  }) async => editor;
}

Map<String, dynamic> snapshot() => {
  'theme': 'white',
  'glass': 'frosted',
  'background': 'ambient',
  'ideas': [
    for (final id in ['a', 'b', 'c'])
      Idea(id, 'body', '灵感', Icons.star, Colors.blue, id: id).toJson(),
  ],
  'completed': <String>[],
};

void main() {
  testWidgets(
    'drag paints new card placement before disk reply, then rolls back failure',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final store = SlowOrderStore();
      final prefs = CardOrderPreferences(store);
      await prefs.restore();
      await t.pumpWidget(
        MorrowApp(
          storage: MemoryStorage()..data = snapshot(),
          cardOrderPreferences: prefs,
        ),
      );
      await t.pumpAndSettle();
      final before = t.getTopLeft(find.byKey(const ValueKey('card-drag:a')));
      await holdMove(
        t,
        find.byKey(const ValueKey('card-drag:a')),
        find.byKey(const ValueKey('card-drag:c')),
      );
      expect(store.writeDone.isCompleted, isFalse);
      expect(t.widget<WorkspaceViewport>(find.byType(WorkspaceViewport)).ids, [
        'b',
        'c',
        'a',
      ]);
      expect(
        t.getTopLeft(find.byKey(const ValueKey('card-drag:a'))),
        isNot(before),
      );
      store.writeDone.completeError(StateError('disk unavailable'));
      await t.pumpAndSettle();
      expect(t.widget<WorkspaceViewport>(find.byType(WorkspaceViewport)).ids, [
        'a',
        'b',
        'c',
      ]);
      expect(prefs.busy, isFalse);
      await t.pumpWidget(const SizedBox());
      prefs.dispose();
    },
  );

  testWidgets(
    'same-view refresh retains confirmed IDs, starts immediately and clears on failure',
    (t) async {
      final backend = QueryBackend(), q = QueryCoordinator(onChanged: () {});
      q.select(backend, conditions(''));
      await t.pump(const Duration(milliseconds: 1));
      expect(backend.requests, hasLength(1));
      backend.requests.single.complete(['a']);
      await t.pump(const Duration(milliseconds: 1));
      q.select(backend, conditions('', generation: 1));
      expect(q.refreshing, isTrue);
      expect(q.ids, ['a']);
      await t.pump(const Duration(milliseconds: 1));
      expect(backend.requests, hasLength(2));
      backend.requests.last.complete(['a', 'b']);
      await t.pump(const Duration(milliseconds: 1));
      expect(q.ids, ['a', 'b']);
      expect(q.refreshing, isFalse);
      q.select(backend, conditions('', generation: 1));
      await t.pump(const Duration(milliseconds: 1));
      expect(backend.requests, hasLength(2));
      q.select(backend, conditions('', generation: 2));
      await t.pump(const Duration(milliseconds: 1));
      backend.requests.last.completeError(
        const QueryFailure('revoked', terminal: true),
      );
      await t.pump(const Duration(milliseconds: 1));
      expect(q.ids, isEmpty);
      expect(q.refreshing, isFalse);
      q.dispose();
    },
  );

  testWidgets(
    'confirmed card edits refresh in place; presentation saves issue no query',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final backend = QueryBackend();
      await t.pumpWidget(
        MorrowApp(
          storage: MemoryStorage()..data = snapshot(),
          workbench: backend,
        ),
      );
      await waitForLocalizedWorkbench(t);
      await t.pump(const Duration(milliseconds: 1));
      backend.requests.single.complete(['a', 'b', 'c']);
      await t.pumpAndSettle();
      final dynamic studio = t.state(find.byType(Studio));
      studio.persist();
      await t.pumpAndSettle();
      expect(backend.requests, hasLength(1));
      await studio.pluginChange(
        PluginAction.favorite,
        (studio.ideas as List<Idea>).first,
        flag: true,
      );
      await t.pump(const Duration(milliseconds: 1));
      await t.pump(const Duration(milliseconds: 1));
      expect(backend.requests, hasLength(2));
      expect(find.byKey(const ValueKey('query-loading')), findsNothing);
      expect(
        find.byKey(const ValueKey('card-drag:a')).hitTestable(),
        findsOneWidget,
      );
      backend.requests.last.complete(['a', 'b', 'c']);
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'card tips preserve paste offsets, multiline import and saved draft',
    (t) async {
      t.view.physicalSize = const Size(1000, 1100);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final editor = EditorFixture();
      final idea = Idea(
        'Card',
        'body',
        '灵感',
        Icons.star,
        Colors.blue,
        id: editor.targetId,
        todos: ['Alpha', 'Beta'],
      );
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: NewIdeaDialog(
              initialIdea: idea,
              editor: editor,
              readClipboard: () async => const PastedContent(text: 'xy\nz'),
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      final field = find.byKey(const ValueKey('tip-item-1'));
      await t.ensureVisible(field);
      await t.pumpAndSettle();
      await t.tap(field);
      t.widget<TextField>(field).controller!.selection = const TextSelection(
        baseOffset: 1,
        extentOffset: 3,
      );
      await t.pump(const Duration(milliseconds: 1));
      final dynamic dialog = t.state(find.byType(NewIdeaDialog));
      await dialog.paste();
      await t.pumpAndSettle();
      expect(editor.events.single.field, 'todos');
      expect(editor.events.single.startUtf16, 7);
      expect(editor.events.single.endUtf16, 9);
      expect(find.byKey(const ValueKey('tip-item-2')), findsOneWidget);
      expect(
        t
            .widget<TextField>(find.byKey(const ValueKey('tip-item-2')))
            .controller!
            .text,
        'za',
      );
      await click(t, 'idea-save');
      expect(editor.fields.single.todos, 'Alpha\nBxy\nza');
      expect(editor.drafts.single.todos, ['Alpha', 'Bxy', 'za']);
      editor.attempts.single.complete(editor.drafts.single);
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets('closing an unchanged editor does not requery the workspace', (
    t,
  ) async {
    t.view.physicalSize = const Size(1440, 1000);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    final idea = Idea(
      'Card',
      'body',
      '灵感',
      Icons.star,
      Colors.blue,
      id: 'editor-card',
      contentRevision: BigInt.one,
    );
    final backend = UnchangedEditorBackend(idea);
    final data = snapshot()..['workspaceIdeas'] = [idea];
    await t.pumpWidget(
      MorrowApp(storage: MemoryStorage()..data = data, workbench: backend),
    );
    await waitForLocalizedWorkbench(t);
    await t.pump(const Duration(milliseconds: 1));
    backend.requests.single.complete([idea.id]);
    await t.pumpAndSettle();
    final dynamic studio = t.state(find.byType(Studio));
    final Future<void> opening = studio.openIdea(idea, initialAction: 'edit');
    await t.pumpAndSettle();
    expect(find.byType(NewIdeaDialog), findsOneWidget);
    Navigator.of(t.element(find.byType(NewIdeaDialog))).pop();
    await t.pumpAndSettle();
    await opening;
    await t.pumpAndSettle();
    expect(backend.requests, hasLength(1));
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'card tips fit narrow windows, keep per-row controls and enforce total limit',
    (t) async {
      t.view.physicalSize = const Size(390, 844);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final boundary = GlobalKey(), focus = FocusNode();
      final controller = TextEditingController(text: '喝一杯水\n记下今天的灵感');
      await t.pumpWidget(
        RepaintBoundary(
          key: boundary,
          child: MaterialApp(
            locale: const Locale('zh'),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Scaffold(
              body: SingleChildScrollView(
                padding: const EdgeInsets.all(20),
                child: CardTipsEditor(
                  controller: controller,
                  focusNode: focus,
                  enabled: true,
                  title: '卡片内的小提示',
                ),
              ),
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      expect(t.takeException(), isNull);
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      await t.runAsync(() async {
        final bitmap = await render.toImage();
        final bytes = await bitmap.toByteData(format: ui.ImageByteFormat.png);
        final native = t.binding.runtimeType.toString().contains('Integration');
        await File(
          'build/card-tips-${native ? 'windows' : 'widget'}-390.png',
        ).writeAsBytes(bytes!.buffer.asUint8List());
        bitmap.dispose();
      });
      controller.text = 'a' * 1000;
      await t.pumpAndSettle();
      await t.enterText(find.byKey(const ValueKey('tip-item-0')), 'b' * 1001);
      await t.pumpAndSettle();
      expect(controller.text.length, 1000);
      expect(
        t.widget<FilledButton>(find.byKey(const ValueKey('tip-add'))).onPressed,
        isNull,
      );
      await t.pumpWidget(const SizedBox());
      controller.dispose();
      focus.dispose();
    },
  );

  testWidgets(
    'close hides immediately, drains writes before service close, then waits for real exit',
    (t) async {
      final writes = PendingUiWrites(),
          write = Completer<void>(),
          exit = Completer<int>();
      writes.track(write.future);
      final session = SessionCoordinator();
      var hides = 0, closes = 0, destroys = 0;
      await session.run(
        () async => session.attach(
          process: Object(),
          library: 'test',
          exited: exit.future,
          close: () async {
            closes++;
            await exit.future;
          },
        ),
      );
      final close = ApplicationShutdown(
        session: session,
        hideOnRequest: true,
        prepareClose: writes.drain,
        showClosing: () {},
        hideWindow: () async => hides++,
        showWindow: () async {},
        destroyWindow: () async => destroys++,
      );
      session.addListener(close.observe);
      close.request();
      await t.pump(const Duration(milliseconds: 1));
      expect(hides, 1);
      expect(closes, 0);
      write.complete();
      await t.pump(const Duration(milliseconds: 1));
      expect(closes, 1);
      expect(destroys, 0);
      close.request();
      await t.pump(const Duration(milliseconds: 1));
      expect(closes, 1);
      exit.complete(0);
      await t.pump(const Duration(milliseconds: 1));
      expect(destroys, 1);
      session.dispose();
    },
  );

  testWidgets(
    'closing surface keeps editor mounted until writes settle and hides old input',
    (t) async {
      final key = GlobalKey();
      Widget frame(bool closing, bool retired) => MaterialApp(
        home: Material(
          child: ApplicationCloseSurface(
            closing: closing,
            retired: retired,
            closingView: const Text('Closing'),
            child: TextField(key: key),
          ),
        ),
      );
      await t.pumpWidget(frame(false, false));
      final state = key.currentState;
      await t.pumpWidget(frame(true, false));
      expect(key.currentState, same(state));
      expect(find.byType(TextField).hitTestable(), findsNothing);
      await t.pumpWidget(frame(true, true));
      expect(key.currentState, isNull);
    },
  );
}
