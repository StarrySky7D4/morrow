import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/plugins/workbench_loading.dart';
import 'package:morrow_studio/plugins/workbench_startup.dart';

class ObservedStorage extends MemoryStorage {
  int writes = 0;
  @override
  Future<void> write(Map<String, dynamic> value) async {
    writes++;
    await super.write(value);
  }
}

void main() {
  testWidgets('Loading stays localized and fits a narrow window', (t) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(320, 480);
    addTearDown(t.view.reset);
    for (final code in ['ru', 'fr', 'de', 'es', 'ja', 'ko', 'pt']) {
      await t.pumpWidget(WorkbenchLoading(locale: Locale(code)));
      for (var i = 0; i < 15; i++) {
        await t.pump(const Duration(milliseconds: 20));
      }
      expect(
        find.text(L10n.forLocale(Locale(code)).visualLoading),
        findsOneWidget,
      );
      expect(find.byType(LinearProgressIndicator), findsOneWidget);
      expect(find.byType(TextField), findsNothing);
      expect(t.takeException(), isNull);
    }
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'Loading to workbench preserves restored data without saving a placeholder',
    (t) async {
      final storage = ObservedStorage()
        ..data = {
          'theme': 'dark',
          'glass': 'clear',
          'background': 'solid',
          'uiLocale': 'ru',
          'ideas': [
            Idea(
              'Preserved card',
              'Draft',
              '灵感',
              Icons.star,
              Colors.blue,
              id: 'preserved',
            ).toJson(),
          ],
        };
      final original = storage.data;
      final loaded = ValueNotifier(false);
      addTearDown(loaded.dispose);
      var reveals = 0;
      await t.pumpWidget(
        WorkbenchStartup(ready: loaded, locale: const Locale('ru')),
      );
      await t.pump(const Duration(milliseconds: 200));
      expect(storage.writes, 0);
      var ready = 0;
      await t.pumpWidget(
        WorkbenchStartup(
          ready: loaded,
          locale: const Locale('ru'),
          onRevealed: () => reveals++,
          child: MorrowApp(
            storage: storage,
            onFirstFrame: () {
              ready++;
              loaded.value = true;
            },
          ),
        ),
      );
      await t.pumpAndSettle();
      expect(ready, 1);
      expect(reveals, 1);
      expect(storage.writes, 0);
      expect(storage.data, original);
      expect(find.byKey(const ValueKey('startup-loading')), findsNothing);
      expect(
        t.widget<Studio>(find.byType(Studio)).palette.theme,
        StudioTheme.dark,
      );
      expect(
        t.widget<Studio>(find.byType(Studio)).restored!['ideas'],
        original!['ideas'],
      );
      await t.pump();
      expect(ready, 1);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Startup keeps its cover until ready and fades without remounting',
    (t) async {
      final ready = ValueNotifier(false);
      addTearDown(ready.dispose);
      var reveals = 0;
      await t.pumpWidget(WorkbenchStartup(ready: ready));
      await t.pump(const Duration(milliseconds: 100));
      final loadingState = t.state(find.byType(MaterialApp));
      final focus = FocusNode();
      addTearDown(focus.dispose);
      final content = MaterialApp(
        home: Scaffold(body: TextField(focusNode: focus)),
      );
      await t.pumpWidget(
        WorkbenchStartup(
          ready: ready,
          child: content,
          onRevealed: () => reveals++,
        ),
      );
      final loadingApp = find.descendant(
        of: find.byType(WorkbenchLoading),
        matching: find.byType(MaterialApp),
      );
      expect(t.state(loadingApp), same(loadingState));
      final fieldState = t.state(find.byType(TextField));
      await t.pump(const Duration(milliseconds: 500));
      expect(find.byType(WorkbenchLoading), findsOneWidget);
      expect(focus.canRequestFocus, isFalse);
      ready.value = true;
      await t.pump();
      await t.pump();
      await t.pump(const Duration(milliseconds: 70));
      final fade = t.widget<FadeTransition>(
        find
            .ancestor(
              of: find.byType(WorkbenchLoading),
              matching: find.byType(FadeTransition),
            )
            .first,
      );
      expect(fade.opacity.value, greaterThan(0));
      expect(fade.opacity.value, lessThan(1));
      // The restored workbench is usable while the cover finishes fading.
      expect(focus.canRequestFocus, isTrue);
      await t.pumpAndSettle();
      expect(find.byType(WorkbenchLoading), findsNothing);
      expect(t.state(find.byType(TextField)), same(fieldState));
      expect(focus.canRequestFocus, isTrue);
      expect(reveals, 1);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'Reduced motion reveals immediately and pending disposal is safe',
    (t) async {
      t.platformDispatcher.accessibilityFeaturesTestValue =
          const FakeAccessibilityFeatures(disableAnimations: true);
      addTearDown(t.platformDispatcher.clearAccessibilityFeaturesTestValue);
      final ready = ValueNotifier(false);
      addTearDown(ready.dispose);
      await t.pumpWidget(
        WorkbenchStartup(
          ready: ready,
          child: const ColoredBox(color: Colors.black),
        ),
      );
      ready.value = true;
      await t.pump();
      await t.pump();
      expect(find.byType(WorkbenchLoading), findsNothing);
      await t.pumpWidget(const SizedBox());
      ready.value = false;
      await t.pumpWidget(
        WorkbenchStartup(ready: ready, child: const SizedBox()),
      );
      ready.value = true;
      await t.pumpWidget(const SizedBox());
      await t.pump();
      expect(t.takeException(), isNull);
    },
  );
}
