import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/card_order_preferences.dart';
import 'package:morrow_studio/hold_reorder.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/tip_preferences.dart';
import 'package:morrow_studio/tip_list_editor.dart';
import 'package:morrow_studio/workspace_viewport.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/music/music_panel.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'music_test.dart' show FakeAudio, track;
import 'context_menu_workflow_test.dart' show secondaryClick, chooseMenu;

Future<void> holdMove(
  WidgetTester t,
  Finder source,
  Finder target, {
  bool after = true,
  PointerDeviceKind kind = PointerDeviceKind.mouse,
}) async {
  final g = await t.startGesture(t.getCenter(source), kind: kind);
  await t.pump(const Duration(milliseconds: 450));
  final rect = t.getRect(target);
  await g.moveTo(
    Offset(rect.center.dx, after ? rect.bottom - 5 : rect.top + 5),
  );
  await t.pump(const Duration(milliseconds: 20));
  await g.up();
  await g.removePointer();
  await t.pumpAndSettle();
}

class _FailingStore extends MemoryTipStore {
  bool fail = false;
  @override
  Future<void> write(String value) async {
    if (fail) throw StateError('unavailable');
    await super.write(value);
  }
}

void main() {
  testWidgets('playlist handles and context commands reorder the actual list', (
    t,
  ) async {
    final music = MusicController(
      tracks: [track('Alpha'), track('Beta')],
      createTransport: FakeAudio.new,
    );
    await t.pumpWidget(
      MaterialApp(
        locale: const Locale('en'),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: SizedBox(width: 300, child: MusicPanel(controller: music)),
          ),
        ),
      ),
    );
    await t.pumpAndSettle();
    await secondaryClick(t, find.text('Music player'));
    await chooseMenu(t, 'Expand playlist');
    final rows = find.byType(HoldReorder);
    final source = find.descendant(
      of: rows.first,
      matching: find.byWidgetPredicate(
        (w) => w is SizedBox && w.key.toString().contains('drag-handle:'),
      ),
    );
    await holdMove(t, source, rows.last);
    expect(music.tracks.map((e) => e.title), ['Beta', 'Alpha']);
    expect(music.current!.title, 'Alpha');
    await secondaryClick(
      t,
      find.descendant(of: rows.last, matching: find.text('Alpha')),
    );
    await chooseMenu(t, 'Move track up');
    expect(music.tracks.map((e) => e.title), ['Alpha', 'Beta']);
    await t.pumpWidget(const SizedBox());
    music.dispose();
  });
  test(
    'card order persists per page, filters preserve hidden slots and failures roll back',
    () async {
      final store = _FailingStore();
      final order = CardOrderPreferences(store);
      await order.restore();
      await order.move(
        'all',
        'a',
        'c',
        true,
        all: ['a', 'hidden', 'b', 'c'],
        visible: ['a', 'b', 'c'],
      );
      expect(order.order('all', ['a', 'hidden', 'b', 'c']), [
        'b',
        'hidden',
        'c',
        'a',
      ]);
      expect(order.manual('all'), isTrue);
      expect(order.manual('favorites'), isFalse);
      await order.select('all', false);
      final reopened = CardOrderPreferences(store);
      await reopened.restore();
      expect(reopened.manual('all'), isFalse);
      expect(reopened.order('all', ['a', 'b', 'new', 'c']), [
        'b',
        'c',
        'a',
        'new',
      ]);
      expect(reopened.order('favorites', ['a', 'b', 'c']), ['a', 'b', 'c']);
      store.fail = true;
      await expectLater(
        order.move(
          'all',
          'b',
          'a',
          true,
          all: ['a', 'hidden', 'b', 'c'],
          visible: ['b', 'hidden', 'c', 'a'],
        ),
        throwsStateError,
      );
      expect(order.order('all', ['a', 'hidden', 'b', 'c']), [
        'b',
        'hidden',
        'c',
        'a',
      ]);
      expect(order.manual('all'), isFalse);
      expect(order.busy, isFalse);
      order.dispose();
      reopened.dispose();
    },
  );

  testWidgets(
    'long press works with mouse and touch; canceled, stale and cross-list drops are ignored',
    (t) async {
      final scope = Object(), other = Object();
      var revision = Object();
      var calls = 0;
      Widget frame({bool foreign = false}) => MaterialApp(
        home: Scaffold(
          body: Column(
            children: [
              for (final id in ['a', 'b'])
                HoldReorder(
                  key: ValueKey(id),
                  scope: foreign && id == 'b' ? other : scope,
                  id: id,
                  revision: revision,
                  label: id,
                  enabled: true,
                  onMove: (source, target, after) {
                    expect([source, target, after], ['a', 'b', true]);
                    calls++;
                  },
                  child: SizedBox(width: 250, height: 100, child: Text(id)),
                ),
            ],
          ),
        ),
      );
      await t.pumpWidget(frame());
      for (final kind in [PointerDeviceKind.mouse, PointerDeviceKind.touch]) {
        await holdMove(
          t,
          find.byKey(const ValueKey('a')),
          find.byKey(const ValueKey('b')),
          kind: kind,
        );
      }
      expect(calls, 2);
      var g = await t.startGesture(
        t.getCenter(find.byKey(const ValueKey('a'))),
      );
      await t.pump(const Duration(milliseconds: 450));
      await g.moveTo(
        t.getBottomRight(find.byKey(const ValueKey('b'))) - const Offset(5, 5),
      );
      await g.cancel();
      await t.pumpAndSettle();
      expect(calls, 2);
      g = await t.startGesture(t.getCenter(find.byKey(const ValueKey('a'))));
      await t.pump(const Duration(milliseconds: 450));
      revision = Object();
      await t.pumpWidget(frame());
      await g.moveTo(t.getCenter(find.byKey(const ValueKey('b'))));
      await g.up();
      await t.pumpAndSettle();
      expect(calls, 2);
      await t.pumpWidget(frame(foreign: true));
      await holdMove(
        t,
        find.byKey(const ValueKey('a')),
        find.byKey(const ValueKey('b')),
      );
      expect(calls, 2);
    },
  );

  testWidgets(
    'edge scrolling continues across virtualized rows and stops on cancel',
    (t) async {
      final scroll = ScrollController(), scope = Object(), revision = Object();
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              height: 300,
              child: ListView.builder(
                controller: scroll,
                itemCount: 60,
                itemExtent: 80,
                itemBuilder: (_, i) => HoldReorder(
                  scope: scope,
                  id: i,
                  revision: revision,
                  label: '$i',
                  enabled: true,
                  onMove: (_, _, _) {},
                  child: Text('row-$i'),
                ),
              ),
            ),
          ),
        ),
      );
      final g = await t.startGesture(t.getCenter(find.text('row-0')));
      await t.pump(const Duration(milliseconds: 450));
      await g.moveTo(const Offset(100, 285));
      for (var i = 0; i < 35; i++) {
        await t.pump(const Duration(milliseconds: 32));
      }
      expect(scroll.offset, greaterThan(400));
      await g.cancel();
      await t.pumpAndSettle();
      final stopped = scroll.offset;
      await t.pump(const Duration(milliseconds: 300));
      expect(scroll.offset, stopped);
      await t.pumpWidget(const SizedBox());
      scroll.dispose();
    },
  );

  testWidgets(
    'actual workbench cards drag, change mode and restore saved order',
    (t) async {
      t.view.physicalSize = const Size(1440, 1000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final storage = MemoryStorage()
        ..data = {
          'version': 1,
          'theme': 'mist',
          'glass': 'frosted',
          'background': 'solid',
          'ideas': [
            for (final id in ['a', 'b', 'c'])
              Idea(id, 'body', '灵感', Icons.star, Colors.blue, id: id).toJson(),
          ],
          'completed': <String>[],
        };
      final store = MemoryTipStore();
      // Use a retained store to verify reopening independently of the widget tree.
      final saved = CardOrderPreferences(store);
      await saved.restore();
      await t.pumpWidget(
        MorrowApp(
          storage: storage,
          cardOrderPreferences: saved,
          initialLocale: const Locale('en'),
        ),
      );
      await t.pumpAndSettle();
      await holdMove(
        t,
        find.byKey(const ValueKey('card-drag:a')),
        find.byKey(const ValueKey('card-drag:c')),
      );
      expect(t.widget<WorkspaceViewport>(find.byType(WorkspaceViewport)).ids, [
        'b',
        'c',
        'a',
      ]);
      expect(find.text('Custom order'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      await t.pumpAndSettle();
      final reopened = CardOrderPreferences(store);
      await reopened.restore();
      await t.pumpWidget(
        MorrowApp(
          storage: storage,
          cardOrderPreferences: reopened,
          initialLocale: const Locale('en'),
        ),
      );
      await t.pumpAndSettle();
      expect(t.widget<WorkspaceViewport>(find.byType(WorkspaceViewport)).ids, [
        'b',
        'c',
        'a',
      ]);
      await t.tap(find.byKey(const ValueKey('workbench-sort')));
      await t.pumpAndSettle();
      final titleItem = find.byWidgetPredicate(
        (w) => w is PopupMenuItem<String> && w.value == 'title',
      );
      await t.tap(titleItem);
      await t.pumpAndSettle();
      expect(t.widget<WorkspaceViewport>(find.byType(WorkspaceViewport)).ids, [
        'a',
        'b',
        'c',
      ]);
      await t.pumpWidget(const SizedBox());
      saved.dispose();
      reopened.dispose();
    },
  );

  testWidgets(
    'tip handles reorder drafts while text and identity stay together',
    (t) async {
      List<TipItem>? draft;
      final items = [const TipItem('a', 'Alpha'), const TipItem('b', 'Beta')];
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: TipListEditor(
                initial: items,
                defaults: items,
                daily: true,
                onChanged: (value, _) => draft = value,
              ),
            ),
          ),
        ),
      );
      await t.enterText(
        find.byKey(const ValueKey('tip-item-0')),
        'Edited Alpha',
      );
      await t.pumpAndSettle();
      await holdMove(
        t,
        find.byKey(const ValueKey('drag-handle:a')),
        find.byType(HoldReorder).last,
      );
      expect(draft!.map((v) => v.id), ['b', 'a']);
      expect(draft!.map((v) => v.text), ['Beta', 'Edited Alpha']);
      expect(
        t
            .widget<TextField>(find.byKey(const ValueKey('tip-item-1')))
            .controller!
            .text,
        'Edited Alpha',
      );
    },
  );

  test(
    'playlist reorder preserves playing track, position and persisted selection',
    () async {
      final audio = FakeAudio();
      var saves = 0;
      final a = track('a'), b = track('b'), c = track('c');
      final music = MusicController(
        tracks: [a, b, c],
        createTransport: () => audio,
        resolve: (s) async => ResolvedTexture(uri: s.location),
        onSave: () => saves++,
      );
      await music.select(0);
      await music.seek(const Duration(seconds: 4));
      final plays = audio.playCount;
      music.reorder(a, c, true);
      expect(music.tracks, [b, c, a]);
      expect(music.current, same(a));
      expect(music.index, 2);
      expect(music.position, const Duration(seconds: 4));
      expect(music.playing, isTrue);
      expect(audio.uri, 'a');
      expect(audio.playCount, plays);
      final data = music.toJson();
      expect(data['index'], 2);
      expect((data['tracks'] as List).map((v) => v['source']['location']), [
        'b',
        'c',
        'a',
      ]);
      expect(saves, greaterThan(0));
      music.dispose();
    },
  );
}
