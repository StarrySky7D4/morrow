import 'dart:async';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/plugins/workbench_labels.dart';
import 'package:morrow_studio/plugins/generated/workbench.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as identity;

typedef Query = ({
  String section,
  String filter,
  String text,
  String sort,
  String? operation,
});
Uint8List encode(Query query) {
  final message = MessageBuilder();
  final request = message.initRoot(wire.requestFactory);
  request.version = 1;
  request.digest = Uint8List.fromList(identity.workbenchDigest);
  request.action = wire.Action.query;
  request.section = query.section;
  request.filter = query.filter;
  request.text = query.text;
  request.sort = query.sort;
  return message.serialize();
}

class Backend extends WorkbenchBackend {
  final calls = <Query>[];
  final pending = <Completer<List<String>>>[];
  @override
  bool get writable => true;
  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) {
    calls.add((
      section: section,
      filter: filter,
      text: text,
      sort: sort,
      operation: operation,
    ));
    final result = Completer<List<String>>();
    pending.add(result);
    return result.future;
  }

  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) async => idea;
}

void viewport(WidgetTester tester) {
  tester.view.physicalSize = const Size(1500, 1200);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

Future<void> settleQuery(WidgetTester tester, Backend backend) async {
  backend.pending.last.complete([]);
  await tester.pump(const Duration(milliseconds: 1));
  await tester.pump(const Duration(milliseconds: 400));
}

final changedLabels = WorkbenchLabels(
  pages: {
    WorkbenchPage.overview: 'Dashboard',
    WorkbenchPage.inbox: 'Same label',
    WorkbenchPage.projects: 'Same label',
    WorkbenchPage.laboratory: 'Experiments',
    WorkbenchPage.favorites: 'Bookmarks',
  },
  filters: {
    GeneralFilter.all: 'Everything',
    StageFilter(WorkbenchStage.planned): 'Same stage label',
    StageFilter(WorkbenchStage.active): 'Same stage label',
    StageFilter(WorkbenchStage.completed): 'Finished',
  },
  sorts: {WorkbenchSort.title: 'Title order', WorkbenchSort.recent: 'Recent'},
);

void main() {
  test(
    'all existing page/filter/sort combinations retain exact v1 query frames',
    () {
      // Independent legacy vectors: the adapter must keep these original wire tokens.
      const pages = ['概览', '灵感收件箱', '小项目', '实验室', '已收藏'];
      const filters = [
        ['全部', '有待办', '含附件', '仅收藏'],
        ['全部', '待整理', '已整理'],
        ['全部', '计划中', '推进中', '已完成'],
        ['全部', '待验证', '验证中', '已记录'],
        ['全部', '图像', '音视频', '文件', '文字'],
      ];
      const sorts = ['最近添加', '收藏优先', '标题排序'];
      final ids = <String>{};
      for (final page in WorkbenchPage.values) {
        expect(ids.add(page.id), isTrue);
        final pageIndex = WorkbenchPage.values.indexOf(page);
        expect(page.filters, hasLength(filters[pageIndex].length));
        expect(
          WorkbenchV1.summaryComponentId(page),
          'summary:${pages[pageIndex]}',
        );
        for (var i = 0; i < page.filters.length; i++) {
          for (var j = 0; j < WorkbenchSort.values.length; j++) {
            for (final text in ['', '😀|中 文', 'a/b|c']) {
              final actual = (
                section: WorkbenchV1.section(page),
                filter: WorkbenchV1.filter(page.filters[i]),
                text: text,
                sort: WorkbenchV1.sort(WorkbenchSort.values[j]),
                operation: 'unchanged',
              );
              final old = (
                section: pages[pageIndex],
                filter: filters[pageIndex][i],
                text: text,
                sort: sorts[j],
                operation: 'unchanged',
              );
              expect(encode(actual), orderedEquals(encode(old)));
            }
          }
        }
      }
      expect(
        StageFilter(WorkbenchStage.planned),
        const StageFilter(WorkbenchStage.planned),
      );
      expect(
        StageFilter(WorkbenchStage.planned),
        isNot(const StageFilter(WorkbenchStage.active)),
      );
    },
  );

  testWidgets(
    'display changes retain ready results and uncertain operation identity',
    (tester) async {
      viewport(tester);
      final backend = Backend();
      final storage = MemoryStorage();
      final app = MorrowApp(storage: storage, workbench: backend);
      Widget wrap(WorkbenchLabels labels) =>
          WorkbenchLabelsScope(labels: labels, child: app);
      await tester.pumpWidget(wrap(const WorkbenchLabels()));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 250));
      final original = backend.calls.single;
      expect(
        (original.section, original.filter, original.sort),
        ('概览', '全部', '最近添加'),
      );
      final bytes = encode(original);
      await settleQuery(tester, backend);
      await tester.pumpWidget(wrap(changedLabels));
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('Dashboard'), findsWidgets);
      expect(find.text('Recent'), findsOneWidget);
      expect(backend.calls, hasLength(1));
      expect(find.byKey(const ValueKey('query-loading')), findsNothing);

      // A real filter change gets a fresh identity; a subsequent label-only rebuild does not.
      await tester.tap(
        find.byKey(ValueKey('filter-${GeneralFilter.pendingTodos.id}')),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.calls, hasLength(2));
      final uncertain = backend.calls.last;
      expect(uncertain.operation, isNot(original.operation));
      backend.pending.last.completeError(
        StateError('transport result unknown'),
      );
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpWidget(wrap(const WorkbenchLabels()));
      await tester.pump(const Duration(milliseconds: 400));
      expect(backend.calls, hasLength(2));
      final retry = find.byKey(const ValueKey('query-retry'));
      await tester.ensureVisible(retry);
      await tester.tap(retry);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 250));
      expect(backend.calls.last.operation, uncertain.operation);
      expect(encode(backend.calls.last), orderedEquals(encode(uncertain)));
      expect(encode(original), orderedEquals(bytes));
      await settleQuery(tester, backend);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('translated duplicate labels navigate and filter by identity', (
    tester,
  ) async {
    viewport(tester);
    final backend = Backend();
    await tester.pumpWidget(
      WorkbenchLabelsScope(
        labels: changedLabels,
        child: MorrowApp(storage: MemoryStorage(), workbench: backend),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    await settleQuery(tester, backend);
    // Both pages deliberately have exactly the same displayed title.
    expect(find.text('Same label'), findsNWidgets(2));
    await tester.tap(find.byKey(ValueKey('nav-${WorkbenchPage.projects.id}')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(backend.calls.last.section, '小项目');
    await settleQuery(tester, backend);
    expect(
      find.byKey(ValueKey('page-${WorkbenchPage.projects.id}')),
      findsOneWidget,
    );
    expect(find.text('Same stage label'), findsNWidgets(2));
    await tester.tap(
      find.byKey(
        ValueKey('filter-${const StageFilter(WorkbenchStage.planned).id}'),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(backend.calls.last.filter, '计划中');
    await settleQuery(tester, backend);
    await tester.tap(
      find.byKey(
        ValueKey('filter-${const StageFilter(WorkbenchStage.active).id}'),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(backend.calls.last.filter, '推进中');
    await settleQuery(tester, backend);
    await tester.tap(find.byKey(const ValueKey('workbench-sort')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Title order').last);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(backend.calls.last.sort, '标题排序');
    await settleQuery(tester, backend);
    await tester.tap(find.byKey(ValueKey('nav-${WorkbenchPage.inbox.id}')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(backend.calls.last.section, '灵感收件箱');
    expect(backend.calls.last.filter, '全部');
    expect(backend.calls.last.sort, '标题排序');
    await settleQuery(tester, backend);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'legacy local records retain category/stage and page component keys',
    (tester) async {
      viewport(tester);
      final original = Idea(
        'Legacy project',
        'body',
        '进行中',
        Idea.icons.first,
        Colors.blue,
        id: 'legacy',
        stage: '计划中',
      ).toJson();
      final storage = MemoryStorage()
        ..data = {
          'theme': 'white',
          'glass': 'frosted',
          'background': 'ambient',
          'ideas': [original],
          'completed': <String>[],
        };
      await tester.pumpWidget(
        WorkbenchLabelsScope(
          labels: changedLabels,
          child: MorrowApp(storage: storage),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(ValueKey('nav-${WorkbenchPage.projects.id}')),
      );
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(
          ValueKey('filter-${const StageFilter(WorkbenchStage.planned).id}'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Legacy project'), findsOneWidget);
      expect(
        find.byWidgetPredicate(
          (widget) => widget is Glass && widget.componentId == 'summary:小项目',
        ),
        findsOneWidget,
      );
      expect(storage.data!['ideas'], [original]);
      await tester.tap(
        find.byKey(
          ValueKey('filter-${const StageFilter(WorkbenchStage.active).id}'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Legacy project'), findsNothing);
      expect(storage.data!['ideas'], [original]);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
