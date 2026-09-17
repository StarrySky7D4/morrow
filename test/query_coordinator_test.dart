import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/plugins/query_coordinator.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';

class QueryBackend extends WorkbenchBackend {
  @override
  bool writable = true;
  final texts = <String>[];
  final operations = <String?>[];
  final requests = <Completer<List<String>>>[];
  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) {
    texts.add(text);
    operations.add(operation);
    final request = Completer<List<String>>();
    requests.add(request);
    return request.future;
  }

  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) async => idea;
}

QueryConditions conditions(String text, {int generation = 0}) => (
  contentGeneration: generation,
  section: WorkbenchPage.overview,
  filter: GeneralFilter.all,
  text: text,
  sort: WorkbenchSort.recent,
);

Future<void> waitForLocalizedWorkbench(WidgetTester tester) async {
  // Loading the verified language pack is independent of query execution.
  // Wait only for the first localized screen, not for the pending guest reply.
  for (
    var attempt = 0;
    attempt < 100 && find.byType(Studio).evaluate().isEmpty;
    attempt++
  ) {
    await tester.pump(const Duration(milliseconds: 10));
  }
  expect(find.byType(Studio), findsOneWidget);
}

void main() {
  testWidgets('debounce coalesces input and A B A never accepts old A', (
    tester,
  ) async {
    final backend = QueryBackend();
    var changes = 0;
    final q = QueryCoordinator(onChanged: () => changes++);
    q.select(backend, conditions('A'));
    await tester.pump(const Duration(milliseconds: 200));
    expect(backend.texts, ['A']);
    q.select(backend, conditions('B'));
    await tester.pump(const Duration(milliseconds: 200));
    q.select(backend, conditions('A'));
    await tester.pump(const Duration(milliseconds: 200));
    expect(backend.texts, ['A']);
    backend.requests[0].complete(['old']);
    await tester.pump(const Duration(milliseconds: 1));
    expect(q.ids, isEmpty);
    expect(changes, 0);
    expect(backend.texts, ['A', 'A']);
    expect(backend.operations[0], isNotNull);
    expect(backend.operations[1], isNot(backend.operations[0]));
    backend.requests[1].complete(['new']);
    await tester.pump(const Duration(milliseconds: 1));
    expect(q.ids, ['new']);
    q.select(backend, conditions('C'));
    await tester.pump(const Duration(milliseconds: 50));
    q.select(backend, conditions('D'));
    await tester.pump(const Duration(milliseconds: 200));
    expect(backend.texts, ['A', 'A', 'D']);
    q.dispose();
    backend.requests[2].complete(['late']);
    await tester.pump(const Duration(milliseconds: 1));
    expect(changes, 1);
  });

  testWidgets(
    'failure retries only on demand and identities invalidate results',
    (tester) async {
      final backend = QueryBackend();
      final q = QueryCoordinator(onChanged: () {}, debounce: Duration.zero);
      q.select(backend, conditions('x'));
      await tester.pump(const Duration(milliseconds: 1));
      backend.requests[0].completeError(StateError('failure'));
      await tester.pump(const Duration(milliseconds: 1));
      expect(q.phase, QueryPhase.failed);
      q.select(backend, conditions('x'));
      await tester.pump(const Duration(seconds: 2));
      expect(backend.requests, hasLength(1));
      q.retry();
      await tester.pump(const Duration(milliseconds: 1));
      expect(backend.operations[1], backend.operations[0]);
      backend.requests[1].complete(['retry']);
      await tester.pump(const Duration(milliseconds: 1));
      backend.writable = false;
      q.select(backend, conditions('x'));
      expect(q.ids, isEmpty);
      backend.writable = true;
      q.select(backend, conditions('x'));
      await tester.pump(const Duration(milliseconds: 1));
      expect(backend.requests, hasLength(3));
      expect(backend.operations[2], isNot(backend.operations[1]));
      q.select(backend, conditions('x', generation: 1));
      await tester.pump(const Duration(milliseconds: 1));
      backend.requests[2].complete(['stale-content']);
      await tester.pump(const Duration(milliseconds: 1));
      expect(q.ids, isEmpty);
      expect(backend.operations[3], isNot(backend.operations[2]));
      backend.requests[3].complete(['fresh']);
      await tester.pump(const Duration(milliseconds: 1));
      q.invalidate(); // successful package configuration also invalidates same conditions
      q.select(backend, conditions('x', generation: 1));
      await tester.pump(const Duration(milliseconds: 1));
      expect(backend.requests, hasLength(5));
      final replacement = QueryBackend();
      q.select(replacement, conditions('x', generation: 1));
      await tester.pump(const Duration(milliseconds: 1));
      backend.requests[4].complete(['old-backend']);
      await tester.pump(const Duration(milliseconds: 1));
      expect(q.ids, isEmpty);
      expect(replacement.requests, hasLength(1));
      expect(replacement.operations.single, isNot(backend.operations.last));
      q.dispose();
      replacement.requests.single.completeError(StateError('late failure'));
      await tester.pump(const Duration(milliseconds: 1));
    },
  );

  testWidgets(
    'composition and typed page/filter changes reject prior results',
    (tester) async {
      final backend = QueryBackend();
      final q = QueryCoordinator(onChanged: () {}, debounce: Duration.zero);
      q.select(backend, conditions('汉'), deferred: true);
      await tester.pump(const Duration(seconds: 1));
      expect(backend.requests, isEmpty);
      q.select(backend, conditions('汉'));
      await tester.pump(const Duration(milliseconds: 1));
      backend.requests.single.complete([]);
      await tester.pump(const Duration(milliseconds: 1));
      q.select(backend, (
        contentGeneration: 0,
        section: WorkbenchPage.projects,
        filter: const StageFilter(WorkbenchStage.active),
        text: '',
        sort: WorkbenchSort.recent,
      ));
      await tester.pump(const Duration(milliseconds: 1));
      q.select(backend, (
        contentGeneration: 0,
        section: WorkbenchPage.inbox,
        filter: const StageFilter(WorkbenchStage.organized),
        text: '',
        sort: WorkbenchSort.recent,
      ));
      await tester.pump(const Duration(milliseconds: 1));
      backend.requests[1].complete(['wrong']);
      await tester.pump(const Duration(milliseconds: 1));
      expect(q.ids, isEmpty);
      expect(backend.requests, hasLength(3));
      q.dispose();
      backend.requests.last.complete([]);
      await tester.pump(const Duration(milliseconds: 1));
    },
  );

  testWidgets(
    'production query status exposes real retry and hides stale cards',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1100);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final backend = QueryBackend();
      final storage = MemoryStorage()
        ..data = {
          'theme': 'white',
          'glass': 'frosted',
          'background': 'ambient',
          'ideas': [
            Idea(
              'Confirmed card',
              'body',
              '灵感',
              Idea.icons[0],
              Colors.blue,
              id: 'confirmed',
            ).toJson(),
          ],
          'completed': <String>[],
        };
      await tester.pumpWidget(
        MorrowApp(
          initialLocale: const Locale('zh'),
          storage: storage,
          workbench: backend,
        ),
      );
      await waitForLocalizedWorkbench(tester);
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.requests, hasLength(1));
      expect(find.byKey(const ValueKey('query-loading')), findsOneWidget);
      backend.requests.single.completeError(StateError('query failed'));
      await tester.pump(const Duration(milliseconds: 1));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.byKey(const ValueKey('query-error')), findsOneWidget);
      await tester.pump(const Duration(seconds: 2));
      expect(backend.requests, hasLength(1));
      final retry = find.byKey(const ValueKey('query-retry'));
      await tester.ensureVisible(retry);
      await tester.tap(retry);
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.requests, hasLength(2));
      expect(backend.operations[1], backend.operations[0]);
      backend.requests.last.completeError(
        const QueryFailure('confirmed failed', terminal: true),
      );
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('此次筛选已终止'), findsOneWidget);
      expect(find.text('重新筛选'), findsOneWidget);
      await tester.ensureVisible(retry);
      await tester.tap(retry);
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.requests, hasLength(3));
      expect(backend.operations[2], isNot(backend.operations[1]));
      backend.requests.last.complete(['confirmed']);
      await tester.pump(const Duration(milliseconds: 1));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.byKey(const ValueKey('query-error')), findsNothing);
      expect(find.text('Confirmed card'), findsOneWidget);
      final field = find.descendant(
        of: find.byKey(const ValueKey('header-search')),
        matching: find.byType(TextField),
      );
      await tester.enterText(field, 'a');
      await tester.pump(const Duration(milliseconds: 50));
      await tester.enterText(field, 'ab');
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.texts, ['', '', '', 'ab']);
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('Confirmed card'), findsNothing);
      expect(find.byKey(const ValueKey('query-loading')), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      backend.requests.last.complete(['disposed']);
      await tester.pump(const Duration(milliseconds: 1));
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'capacity status preserves contents and distinguishes uncertain retry',
    (tester) async {
      tester.view.physicalSize = const Size(1440, 1100);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final backend = QueryBackend();
      await tester.pumpWidget(
        MorrowApp(
          initialLocale: const Locale('zh'),
          storage: MemoryStorage(),
          workbench: backend,
        ),
      );
      await waitForLocalizedWorkbench(tester);
      await tester.pump(const Duration(milliseconds: 250));
      backend.requests.last.completeError(
        const QueryFailure('capacity', terminal: true, capacity: true),
      );
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('查询历史容量已满'), findsOneWidget);
      expect(find.text('已有内容已保留。此版本尚不支持清理查询历史。'), findsOneWidget);
      expect(find.text('重新检查'), findsOneWidget);
      await tester.pump(const Duration(seconds: 2));
      expect(backend.requests, hasLength(1));
      final retry = find.byKey(const ValueKey('query-retry'));
      await tester.ensureVisible(retry);
      await tester.tap(retry);
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.operations[1], isNot(backend.operations[0]));
      backend.requests.last.completeError(
        const QueryFailure(
          'capacity uncertain',
          terminal: false,
          capacity: true,
        ),
      );
      await tester.pump(const Duration(milliseconds: 300));
      await tester.ensureVisible(retry);
      await tester.tap(retry);
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.operations[2], backend.operations[1]);
      backend.requests.last.complete([]);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
      expect(tester.takeException(), isNull);
    },
  );
}
