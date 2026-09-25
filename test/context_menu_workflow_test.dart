import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/component_context_menu.dart';
import 'package:morrow_studio/component_material_page.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/music/music_controller.dart';
import 'package:morrow_studio/music/music_panel.dart';
import 'package:morrow_studio/storage.dart';
import 'music_test.dart' show FakeAudio, track;

Future<void> secondaryClick(WidgetTester t, Finder target) async {
  await t.ensureVisible(target);
  await t.pumpAndSettle();
  final mouse = await t.createGesture(
    kind: PointerDeviceKind.mouse,
    buttons: kSecondaryMouseButton,
  );
  await mouse.down(t.getCenter(target));
  await mouse.up();
  await mouse.removePointer();
  await t.pumpAndSettle();
}

Future<void> chooseMenu(WidgetTester t, String label) async {
  await t.tap(
    find
        .descendant(
          of: find.byType(PopupMenuItem<int>),
          matching: find.text(label),
        )
        .last,
  );
  await t.pumpAndSettle();
}

void main() {
  testWidgets(
    'keyboard menu works on plain surfaces; dismissal does not execute',
    (t) async {
      var selected = 0;
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ComponentContextMenu(
              actions: () => [
                ComponentMenuAction(
                  label: 'Command',
                  icon: Icons.add,
                  onSelected: () => selected++,
                ),
              ],
              child: const SizedBox(
                width: 220,
                height: 120,
                child: Text('Surface'),
              ),
            ),
          ),
        ),
      );
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.f10);
      await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await t.pumpAndSettle();
      expect(find.text('Command'), findsOneWidget);
      await t.sendKeyEvent(LogicalKeyboardKey.escape);
      await t.pumpAndSettle();
      expect(selected, 0);
      await t.sendKeyEvent(LogicalKeyboardKey.contextMenu);
      await t.pumpAndSettle();
      await chooseMenu(t, 'Command');
      expect(selected, 1);
    },
  );

  testWidgets('open menu rechecks permission before executing a command', (
    t,
  ) async {
    var allowed = true;
    var calls = 0;
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ComponentContextMenu(
            actions: () => [
              ComponentMenuAction(
                label: 'Change',
                icon: Icons.edit,
                isEnabled: () => allowed,
                onSelected: () => calls++,
              ),
            ],
            child: const SizedBox(
              width: 220,
              height: 120,
              child: Text('Surface'),
            ),
          ),
        ),
      ),
    );
    await secondaryClick(t, find.text('Surface'));
    allowed = false;
    await chooseMenu(t, 'Change');
    expect(calls, 0);
    expect(t.takeException(), isNull);
  });

  Future<MemoryStorage> studio(WidgetTester t) async {
    t.view.physicalSize = const Size(1440, 1000);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    final storage = MemoryStorage();
    await storage.write({
      'version': 1,
      'theme': 'white',
      'glass': 'frosted',
      'background': 'ambient',
      'ideas': [
        Idea(
          'Context card',
          'Original body',
          '灵感',
          Idea.icons.first,
          Colors.purple,
          id: 'context-card',
        ).toJson(),
      ],
    });
    await t.pumpWidget(
      MorrowApp(storage: storage, initialLocale: const Locale('en')),
    );
    await t.pumpAndSettle();
    return storage;
  }

  testWidgets('card context edit saves and context delete keeps undo', (
    t,
  ) async {
    final storage = await studio(t);
    await secondaryClick(t, find.text('Context card'));
    await chooseMenu(t, 'Edit');
    expect(find.byType(NewIdeaDialog), findsOneWidget);
    await t.enterText(
      find.byKey(const ValueKey('idea-title')),
      'Edited from context',
    );
    await t.tap(find.byKey(const ValueKey('idea-save')));
    await t.pumpAndSettle();
    expect(
      (storage.data!['ideas'] as List).single['title'],
      'Edited from context',
    );
    await secondaryClick(t, find.text('Edited from context'));
    await chooseMenu(t, 'Delete');
    expect(storage.data!['ideas'], isEmpty);
    await t.tap(find.text('Undo'));
    await t.pumpAndSettle();
    expect(
      (storage.data!['ideas'] as List).single['title'],
      'Edited from context',
    );
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'daily batch commands persist; component menu opens matching settings',
    (t) async {
      final storage = await studio(t);
      final en = L10n.forLocale(const Locale('en'));
      await secondaryClick(t, find.text(en.mainDaily));
      await chooseMenu(t, en.mainTaskMarkComplete);
      expect(storage.data!['completed'], hasLength(3));
      await secondaryClick(t, find.text(en.mainDaily));
      await chooseMenu(t, en.mainTaskMarkIncomplete);
      expect(storage.data!['completed'], isEmpty);
      await secondaryClick(t, find.text(en.mainDaily));
      await chooseMenu(t, en.mainComponentSettings);
      expect(
        t.widget<ComponentMaterialPage>(find.byType(ComponentMaterialPage)).id,
        'daily',
      );
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'playlist row context affects clicked track and preserves current track',
    (t) async {
      final audio = FakeAudio();
      var saved = 0;
      final music = MusicController(
        tracks: [track('first'), track('second')],
        createTransport: () => audio,
        onSave: () => saved++,
      );
      await t.pumpWidget(
        MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: SizedBox(width: 340, child: MusicPanel(controller: music)),
            ),
          ),
        ),
      );
      await t.pumpAndSettle();
      await secondaryClick(t, find.text('Music player'));
      await chooseMenu(t, 'Expand playlist');
      await secondaryClick(t, find.text('second'));
      expect(find.text('Import music'), findsNothing);
      await chooseMenu(t, 'Remove from playlist');
      expect(music.tracks.map((e) => e.title), ['first']);
      expect(music.current!.title, 'first');
      expect(saved, greaterThan(0));
      await t.pumpWidget(const SizedBox());
      music.dispose();
    },
  );
}
