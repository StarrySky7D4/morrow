import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/settings_page_transition.dart';
import 'package:morrow_studio/workspace_viewport.dart';

final _created = <String, int>{};
final _disposed = <String, int>{};

class _Card extends StatefulWidget {
  const _Card(this.id);
  final String id;
  @override
  State<_Card> createState() => _CardState();
}

class _CardState extends State<_Card> {
  @override
  void initState() {
    super.initState();
    _created.update(widget.id, (n) => n + 1, ifAbsent: () => 1);
  }

  @override
  void dispose() {
    _disposed.update(widget.id, (n) => n + 1, ifAbsent: () => 1);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SizedBox(
    height: 110 + (int.tryParse(widget.id) ?? 0) % 3 * 45,
    child: Focus(child: Text(widget.id)),
  );
}

Widget viewport(
  List<String> ids, {
  String page = 'A',
  double width = 700,
  bool pending = false,
  Widget? status,
  Object? session,
}) => MaterialApp(
  home: Center(
    child: SizedBox(
      width: width,
      child: WorkspaceViewport(
        page: page,
        session: session,
        queryPending: pending,
        status: status,
        ids: ids,
        header: const SizedBox(height: 40),
        footer: const SizedBox(height: 40),
        twoColumnWidth: 500,
        itemBuilder: (_, i) => _Card(ids[i]),
      ),
    ),
  ),
);

void main() {
  setUp(() {
    _created.clear();
    _disposed.clear();
  });

  testWidgets('pending host query hides cards, preserves state, fails closed', (
    t,
  ) async {
    const ids = ['0', '1', '2', '3'];
    final session = Object();
    await t.pumpWidget(viewport(ids, session: session));
    await t.pumpAndSettle();
    final original = t.state(
      find.byWidgetPredicate((w) => w is _Card && w.id == '2'),
    );
    await t.pumpWidget(
      viewport(
        [],
        session: session,
        pending: true,
        status: const Text('checking'),
      ),
    );
    await t.pumpAndSettle();
    expect(find.byType(_Card), findsNothing);
    expect(_disposed['2'], isNull);
    expect(
      Focus.of(t.element(find.text('2', skipOffstage: false))).canRequestFocus,
      isFalse,
    );
    await t.pumpWidget(viewport(['3', '2', '1', '0'], session: session));
    await t.pumpAndSettle();
    expect(
      t.state(find.byWidgetPredicate((w) => w is _Card && w.id == '2')),
      same(original),
    );
    await t.pumpWidget(
      viewport([], session: session, status: const Text('denied')),
    );
    await t.pumpAndSettle();
    expect(find.byType(_Card, skipOffstage: false), findsNothing);
    expect(_disposed['2'], 1);
    await t.pumpWidget(viewport(ids, session: session));
    await t.pumpAndSettle();
    final reopened = t.state(
      find.byWidgetPredicate((w) => w is _Card && w.id == '2'),
    );
    await t.pumpWidget(viewport(ids, session: Object()));
    await t.pumpAndSettle();
    expect(
      t.state(find.byWidgetPredicate((w) => w is _Card && w.id == '2')),
      isNot(same(reopened)),
    );
  });

  for (final count in [300, 1000, 10000]) {
    testWidgets('$count variable-height cards use a bounded viewport', (
      t,
    ) async {
      final ids = List.generate(count, (i) => '$i');
      await t.pumpWidget(viewport(ids));
      await t.pumpAndSettle();
      expect(find.byType(_Card).evaluate().length, inInclusiveRange(4, 24));
      expect(_created.length, lessThan(30));
      for (var i = 0; i < 20; i++) {
        await t.drag(find.byType(CustomScrollView), const Offset(0, -400));
        await t.pumpAndSettle();
      }
      expect(find.byType(_Card).evaluate().length, lessThan(30));
      expect(_disposed.length, greaterThan(10));
      await t.pumpWidget(viewport(ids, width: 390));
      await t.pumpAndSettle();
      expect(find.byType(_Card).evaluate().length, lessThan(30));
      expect(t.takeException(), isNull);
    });
  }

  testWidgets('sorting, insert, delete and undo preserve visible card state', (
    t,
  ) async {
    var ids = List.generate(6, (i) => '$i');
    await t.pumpWidget(viewport(ids));
    await t.pumpAndSettle();
    State state(String id) =>
        t.state(find.byWidgetPredicate((w) => w is _Card && w.id == id));
    final original = {for (final id in ids) id: state(id)};
    for (final order in [
      ['0', '2', '1', '4', '3', '5'],
      ['5', '4', '3', '2', '1', '0'],
      ['0', '1', '2', '3', '4', '5'],
    ]) {
      ids = order;
      await t.pumpWidget(viewport(ids));
      await t.pumpAndSettle();
      for (final id in ids) {
        expect(state(id), same(original[id]), reason: id);
      }
      expect(t.takeException(), isNull);
    }
    await t.pumpWidget(viewport(['0', '2', '3', '4', '5']));
    await t.pumpAndSettle();
    expect(state('2'), same(original['2']));
    await t.pumpWidget(viewport(['0', '1', '2', '3', '4', '5']));
    await t.pumpAndSettle();
    expect(state('2'), same(original['2']));
    expect(t.takeException(), isNull);
  });

  testWidgets('page changes keep one tree and restore the scroll position', (
    t,
  ) async {
    final ids = List.generate(300, (i) => '$i');
    await t.pumpWidget(viewport(ids));
    await t.pumpAndSettle();
    await t.drag(find.byType(CustomScrollView), const Offset(0, -650));
    await t.pumpAndSettle();
    final offset = t
        .state<ScrollableState>(find.byType(Scrollable))
        .position
        .pixels;
    for (var i = 0; i < 12; i++) {
      await t.pumpWidget(viewport(ids, page: i.isEven ? 'B' : 'A'));
      await t.pump(const Duration(milliseconds: 20));
      expect(find.byType(CustomScrollView), findsOneWidget);
    }
    await t.pumpAndSettle();
    expect(
      t.state<ScrollableState>(find.byType(Scrollable)).position.pixels,
      closeTo(offset, 1),
    );
    expect(find.byType(_Card).evaluate().length, lessThan(30));
    expect(t.takeException(), isNull);
  });

  testWidgets(
    'settings are lazy, retained once, and hidden tickers are disabled',
    (t) async {
      Widget app(bool open) => MaterialApp(
        home: SettingsPageTransition(
          showSettings: open,
          duration: Duration.zero,
          content: const _Card('content'),
          settings: const _Card('settings'),
        ),
      );
      await t.pumpWidget(app(false));
      expect(_created['settings'], isNull);
      for (var i = 0; i < 6; i++) {
        await t.pumpWidget(app(true));
        await t.pump();
        await t.pumpWidget(app(false));
        await t.pump();
      }
      expect(_created['content'], 1);
      expect(_created['settings'], 1);
      final settings = find.byWidgetPredicate(
        (w) => w is _Card && w.id == 'settings',
        skipOffstage: false,
      );
      expect(TickerMode.valuesOf(t.element(settings)).enabled, isFalse);
      expect(_disposed['settings'], isNull);
      t.binding.handleMemoryPressure();
      await t.pump();
      expect(_disposed['settings'], 1);
      expect(_disposed['content'], isNull);
      await t.pumpWidget(app(true));
      await t.pump();
      expect(_created['settings'], 2);
      t.binding.handleMemoryPressure();
      await t.pump();
      expect(
        _disposed['settings'],
        1,
        reason: 'visible settings stay attached',
      );
      await t.pumpWidget(const SizedBox());
      expect(_disposed['settings'], 2);
    },
  );
}
