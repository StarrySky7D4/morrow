import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/storage.dart';

class _Backend extends WorkbenchBackend {
  @override
  bool get writable => true;
  final calls = <(String, String, String, String, String?)>[];
  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) async {
    calls.add((section, filter, text, sort, operation));
    return [];
  }

  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) async => idea;
}

class _FailingStorage extends MemoryStorage {
  bool fail = false;
  @override
  Future<void> write(Map<String, dynamic> data) async {
    if (fail) throw StateError('storage unavailable');
    await super.write(data);
  }
}

void viewport(WidgetTester t, {double width = 1440}) {
  t.view.devicePixelRatio = 1;
  t.view.physicalSize = Size(width, 950);
  t.binding.platformDispatcher.localesTestValue = const [Locale('zh')];
  addTearDown(t.view.reset);
  addTearDown(t.binding.platformDispatcher.clearLocalesTestValue);
}

Future<void> language(WidgetTester t, String label) async {
  final picker = find.byKey(const ValueKey('language-picker'));
  await t.ensureVisible(picker);
  await t.pumpAndSettle();
  await t.tap(picker);
  await t.pumpAndSettle();
  await t.tap(find.text(label).last);
  await t.pumpAndSettle();
}

void main() {
  testWidgets('language selection preserves query identity and navigation', (
    t,
  ) async {
    viewport(t);
    final backend = _Backend();
    final storage = MemoryStorage();
    await t.pumpWidget(MorrowApp(storage: storage, workbench: backend));
    await t.pumpAndSettle();
    await t.tap(find.byKey(ValueKey('nav-${WorkbenchPage.projects.id}')));
    await t.pumpAndSettle();
    await t.tap(
      find.byKey(
        ValueKey('filter-${const StageFilter(WorkbenchStage.planned).id}'),
      ),
    );
    await t.pumpAndSettle();
    final prior = List.of(backend.calls);
    expect(prior.last.$1, '小项目');
    expect(prior.last.$2, '计划中');
    await language(t, 'English');
    expect(storage.data!['uiLocale'], 'en');
    expect(
      find.byKey(ValueKey('page-${WorkbenchPage.projects.id}')),
      findsOneWidget,
    );
    expect(backend.calls, prior);
    expect(
      find.text(L10n.forLocale(const Locale('en')).mainPageProjects),
      findsWidgets,
    );
    for (final code in ['ru', 'fr', 'de', 'es', 'ja', 'ko', 'pt']) {
      await language(t, L10n.nativeNames[code]!);
      expect(storage.data!['uiLocale'], code);
      expect(backend.calls, prior);
      expect(t.takeException(), isNull, reason: code);
    }
    await language(t, '简体中文');
    expect(storage.data!['uiLocale'], 'zh');
    expect(backend.calls, prior);
    expect(t.takeException(), isNull);
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'system language change keeps editor text IME selection and scroll',
    (t) async {
      viewport(t);
      await t.pumpWidget(MorrowApp(storage: MemoryStorage()));
      await t.pumpAndSettle();
      await t.tap(
        find.text(L10n.forLocale(const Locale('zh')).mainNewIdea).first,
      );
      await t.pumpAndSettle();
      final titleFinder = find.byKey(const ValueKey('idea-title'));
      final title = t.widget<TextField>(titleFinder).controller!;
      const input = TextEditingValue(
        text: '未提交 😀 pinyin',
        selection: TextSelection(baseOffset: 7, extentOffset: 13),
        composing: TextRange(start: 7, end: 13),
      );
      title.value = input;
      final body = t
          .widget<TextField>(find.byKey(const ValueKey('idea-description')))
          .controller!;
      body.text = List.filled(30, '用户正文 stays unchanged').join('\n');
      await t.pumpAndSettle();
      final scroll = t
          .state<ScrollableState>(
            find
                .descendant(
                  of: find.byType(NewIdeaDialog),
                  matching: find.byType(Scrollable),
                )
                .first,
          )
          .position;
      scroll.jumpTo(scroll.maxScrollExtent / 2);
      await t.pumpAndSettle();
      final offset = scroll.pixels;
      for (final code in L10n.nativeNames.keys) {
        t.binding.platformDispatcher.localesTestValue = [Locale(code)];
        await t.pumpAndSettle();
        expect(
          find.text(L10n.forLocale(Locale(code)).mainNewIdeaTitle),
          findsOneWidget,
        );
        expect(t.widget<TextField>(titleFinder).controller, same(title));
        expect(title.value, input);
        expect(body.text, contains('用户正文 stays unchanged'));
        expect(scroll.pixels, closeTo(offset, .1));
        expect(t.takeException(), isNull);
      }
      await t.binding.handlePopRoute();
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'saved locale and preview override do not translate user records',
    (t) async {
      viewport(t);
      final raw = Idea(
        '给灵感一个容器',
        '这是用户自己的原文',
        '灵感',
        Icons.star,
        Colors.blue,
        stage: '用户自定义阶段',
      );
      final storage = MemoryStorage()
        ..data = {
          'version': 1,
          'uiLocale': 'zh',
          'theme': 'white',
          'glass': 'frosted',
          'background': 'ambient',
          'ideas': [raw.toJson()],
          'completed': <String>[],
        };
      await t.pumpWidget(
        MorrowApp(
          storage: storage,
          initialLocale: const Locale('en'),
          initialWarning: '工作台插件不可用，已有内容仍可查看和导出。',
        ),
      );
      await t.pumpAndSettle();
      expect(find.text('给灵感一个容器', findRichText: true), findsWidgets);
      expect(
        find.text(L10n.forLocale(const Locale('en')).recoveryPluginUnavailable),
        findsOneWidget,
      );
      expect(
        t.widget<MaterialApp>(find.byType(MaterialApp)).locale,
        const Locale('en'),
      );
      expect(
        storage.data!['uiLocale'],
        'zh',
      ); // Preview does not itself rewrite preferences.
      expect((storage.data!['ideas'] as List).single['stage'], '用户自定义阶段');
      await t.pumpWidget(const SizedBox());
      storage.data!['uiLocale'] = 'unknown';
      await t.pumpWidget(MorrowApp(storage: storage));
      await t.pumpAndSettle();
      expect(t.widget<MaterialApp>(find.byType(MaterialApp)).locale, isNull);
      expect(
        storage.data!['uiLocale'],
        'unknown',
      ); // Read fallback does not rewrite storage.
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'compact language setting exposes save failure and usable retry',
    (t) async {
      viewport(t, width: 390);
      final storage = _FailingStorage();
      await t.pumpWidget(MorrowApp(storage: storage));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('appearance-toggle')));
      await t.pumpAndSettle();
      storage.fail = true;
      await language(t, 'English');
      // The failed write is surfaced; the visible language remains usable.
      expect(find.textContaining('Could not save'), findsOneWidget);
      storage.fail = false;
      await t.tap(find.text(L10n.forLocale(const Locale('en')).mainRetry).last);
      await t.pumpAndSettle();
      expect(storage.data!['uiLocale'], 'en');
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );
  testWidgets('All locales fit narrow and low windows and restore on reload', (
    t,
  ) async {
    viewport(t);
    for (final code in L10n.nativeNames.keys) {
      for (final width in [1440.0, 1050.0, 800.0, 390.0]) {
        t.view.physicalSize = Size(width, 700);
        await t.pumpWidget(
          MorrowApp(storage: MemoryStorage()..data = {'uiLocale': code}),
        );
        await t.pumpAndSettle();
        expect(
          t.widget<MaterialApp>(find.byType(MaterialApp)).locale,
          Locale(code),
        );
        expect(
          t.takeException(),
          isNull,
          reason: '$code workspace width=$width',
        );
        if (width >= 800) {
          final projects = find.byKey(
            ValueKey('nav-${WorkbenchPage.projects.id}'),
          );
          await t.ensureVisible(projects);
          await t.tap(projects);
          await t.pumpAndSettle();
          expect(t.takeException(), isNull, reason: 'projects width=$width');
        }
        if (width < 1050) {
          await t.tap(find.byKey(const ValueKey('appearance-toggle')));
          await t.pumpAndSettle();
        }
        final picker = find.byKey(const ValueKey('language-picker'));
        await t.ensureVisible(picker);
        await t.pumpAndSettle();
        expect(picker, findsOneWidget);
        expect(
          t.takeException(),
          isNull,
          reason: '$code settings width=$width',
        );
        await t.pumpWidget(const SizedBox());
      }
    }
  });
}
