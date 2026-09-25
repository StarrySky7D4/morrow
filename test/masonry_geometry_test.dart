import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/workspace_viewport.dart';

double height(String id) => 80 + int.parse(id) % 5 * 37;

Widget viewport(List<String> ids, {double width = 700, double extra = 0}) =>
    MaterialApp(
      home: Center(
        child: SizedBox(
          width: width,
          child: WorkspaceViewport(
            page: 'geometry',
            ids: ids,
            header: const SizedBox(height: 40),
            footer: const SizedBox(height: 40),
            twoColumnWidth: 500,
            itemBuilder: (_, i) => LayoutBuilder(
              builder: (_, constraints) => SizedBox(
                key: ValueKey('card-${ids[i]}'),
                height:
                    height(ids[i]) * (constraints.maxWidth < 330 ? 1.4 : 1) +
                    (ids[i] == '0' ? extra : 0),
                child: Text(ids[i]),
              ),
            ),
          ),
        ),
      ),
    );

void expectGeometry(
  WidgetTester t,
  List<String> ids, {
  double width = 700,
  double extra = 0,
}) {
  final scrollable = t.state<ScrollableState>(find.byType(Scrollable));
  final view = t.getRect(find.byType(CustomScrollView));
  final columns = width >= 500 ? 2 : 1;
  final bottoms = List.filled(columns, 0.0);
  final stride = (view.width + 14) / columns;
  for (final id in ids) {
    final column = bottoms.indexOf(bottoms.reduce(min));
    final finder = find.byKey(ValueKey('card-$id'));
    if (finder.evaluate().isNotEmpty) {
      final rect = t.getRect(finder);
      expect(
        rect.left,
        closeTo(view.left + column * stride, .01),
        reason: 'card $id column',
      );
      expect(
        rect.top,
        closeTo(
          view.top + 40 + bottoms[column] - scrollable.position.pixels,
          .01,
        ),
        reason: 'card $id top',
      );
      expect(rect.width, closeTo(stride - 14, .01));
    }
    bottoms[column] +=
        height(id) * (stride - 14 < 330 ? 1.4 : 1) +
        (id == '0' ? extra : 0) +
        14;
  }
  expect(t.takeException(), isNull);
}

void main() {
  testWidgets('two columns stay packed after reorder, edit and width changes', (
    t,
  ) async {
    var ids = List.generate(12, (i) => '$i');
    await t.pumpWidget(viewport(ids));
    await t.pumpAndSettle();
    expectGeometry(t, ids);
    final random = Random(74);
    for (var i = 0; i < 20; i++) {
      ids = [...ids]..shuffle(random);
      await t.pumpWidget(viewport(ids));
      await t.pumpAndSettle();
      expectGeometry(t, ids);
    }
    for (final width in [390.0, 700.0, 610.0, 390.0, 700.0]) {
      await t.pumpWidget(viewport(ids, width: width));
      await t.pumpAndSettle();
      expectGeometry(t, ids, width: width);
    }
    for (final extra in [220.0, 0.0, 100.0]) {
      await t.pumpWidget(viewport(ids, extra: extra));
      await t.pumpAndSettle();
      expectGeometry(t, ids, extra: extra);
    }
    ids = ids.where((id) => int.parse(id).isEven).toList();
    await t.pumpWidget(viewport(ids));
    await t.pumpAndSettle();
    expectGeometry(t, ids);
  });

  testWidgets('scrolled masonry packs both columns after resize and reorder', (
    t,
  ) async {
    var ids = List.generate(300, (i) => '$i');
    await t.pumpWidget(viewport(ids));
    await t.pumpAndSettle();
    for (var i = 0; i < 8; i++) {
      await t.drag(find.byType(CustomScrollView), const Offset(0, -400));
      await t.pumpAndSettle();
      expectGeometry(t, ids);
    }
    for (final width in [390.0, 700.0, 610.0, 700.0]) {
      await t.pumpWidget(viewport(ids, width: width));
      await t.pumpAndSettle();
      expectGeometry(t, ids, width: width);
    }
    ids = [...ids]
      ..insert(25, '299')
      ..removeLast();
    await t.pumpWidget(viewport(ids));
    await t.pumpAndSettle();
    expectGeometry(t, ids);
    t.state<ScrollableState>(find.byType(Scrollable)).position.jumpTo(0);
    await t.pumpAndSettle();
    expectGeometry(t, ids);
  });

  testWidgets(
    'deep jumps, filtered prefixes and empty results rebuild valid positions',
    (t) async {
      var ids = List.generate(1000, (i) => '$i');
      await t.pumpWidget(viewport(ids));
      await t.pumpAndSettle();
      final position = t
          .state<ScrollableState>(find.byType(Scrollable))
          .position;
      for (final offset in [14000.0, 3000.0, 0.0, 8000.0]) {
        position.jumpTo(offset);
        await t.pumpAndSettle();
        expectGeometry(t, ids);
      }
      ids = ids.where((id) => int.parse(id) % 3 == 0).toList();
      await t.pumpWidget(viewport(ids));
      await t.pumpAndSettle();
      expectGeometry(t, ids);
      await t.pumpWidget(viewport([]));
      await t.pumpAndSettle();
      expect(t.takeException(), isNull);
      for (final count in [1, 2, 3, 6]) {
        ids = List.generate(count, (i) => '$i');
        await t.pumpWidget(viewport(ids));
        await t.pumpAndSettle();
        expectGeometry(t, ids);
      }
    },
  );
}
