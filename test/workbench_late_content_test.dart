import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/editor_session.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/storage.dart';

class DeferredWorkbench extends WorkbenchBackend
    implements WorkbenchContentRevisionSource, WorkbenchEditorSupport {
  final pending = <Completer<Idea>>[];
  final refreshes = <Completer<List<Idea>>>[];
  final known = <String, BigInt>{};
  @override
  bool writable = true;
  @override
  Iterable<String> knownContentIds() => known.keys;
  @override
  BigInt? knownContentRevision(String id) => known[id];
  @override
  bool? knownContentDeleted(String id) => known.containsKey(id) ? false : null;
  @override
  Future<List<Idea>> refreshEditorContent() {
    final result = Completer<List<Idea>>();
    refreshes.add(result);
    return result.future;
  }

  @override
  Future<WorkbenchEditorSession> openEditor(
    String target, {
    required bool create,
  }) => throw UnimplementedError();
  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) {
    final reply = Completer<Idea>();
    pending.add(reply);
    return reply.future;
  }

  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) async => const [];
}

class CountingStorage extends MemoryStorage {
  int writes = 0;
  @override
  Future<void> write(Map<String, dynamic> data) async {
    writes++;
    await super.write(data);
  }
}

Idea card(
  String title,
  int revision, {
  bool deleted = false,
  bool historical = false,
}) => Idea(
  title,
  'body',
  '灵感',
  Idea.icons[0],
  Colors.blue,
  id: 'same-card',
  contentRevision: BigInt.from(revision),
  contentDeleted: deleted,
  historicalReceipt: historical,
);

void main() {
  test('signed u64 carrier compares as an unsigned revision', () {
    expect(unsignedContentRevision(-1), (BigInt.one << 64) - BigInt.one);
    expect(
      unsignedContentRevision(-1),
      greaterThan(unsignedContentRevision(1)),
    );
  });

  testWidgets(
    'old workspace reply and historical delete cannot replace current content',
    (tester) async {
      final storage = CountingStorage();
      final old = DeferredWorkbench();
      final current = DeferredWorkbench();
      await tester.pumpWidget(MorrowApp(storage: storage, workbench: old));
      await tester.pumpAndSettle();
      final dynamic studio = tester.state(find.byType(Studio));
      final oldRequest =
          studio.pluginChange(PluginAction.edit, card('old intent', 1))
              as Future<Idea?>;
      expect(old.pending, hasLength(1));
      await tester.pumpWidget(MorrowApp(storage: storage, workbench: current));
      await tester.pump();
      expect(current.refreshes, hasLength(1));
      final writesBeforeCurrent = storage.writes;
      expect(
        (studio.ideas as List<Idea>).where((idea) => idea.id == 'same-card'),
        isEmpty,
      );
      old.pending.single.complete(card('old reply', 2));
      expect(await oldRequest, isNull);
      expect(
        (studio.ideas as List<Idea>).where((idea) => idea.id == 'same-card'),
        isEmpty,
      );

      current.known['same-card'] = BigInt.one;
      current.refreshes.single.complete([]);
      await tester.pump();
      expect(current.refreshes, hasLength(2));
      expect(
        storage.writes,
        writesBeforeCurrent,
        reason: 'an incomplete B refresh must not persist an empty workspace',
      );
      current.refreshes.last.complete([card('workspace B initial', 1)]);
      await tester.pump();
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'workspace B initial',
      );

      final currentRequest =
          studio.pluginChange(PluginAction.edit, card('current intent', 0))
              as Future<Idea?>;
      current.known['same-card'] = BigInt.from(3);
      current.pending.single.complete(card('current value', 3));
      expect((await currentRequest)?.title, 'current value');
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'current value',
      );

      final deleteRequest =
          studio.pluginChange(PluginAction.delete, card('current value', 3))
              as Future<Idea?>;
      current.known['same-card'] = BigInt.from(4);
      current.pending.last.complete(
        card('newer restored value', 4, historical: true),
      );
      expect((await deleteRequest)?.historicalReceipt, isTrue);
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'newer restored value',
      );

      final lateRequest =
          studio.pluginChange(PluginAction.edit, card('late intent', 4))
              as Future<Idea?>;
      current.known['same-card'] = BigInt.from(6);
      current.pending.last.complete(card('late old reply', 5));
      expect(await lateRequest, isNull);
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'newer restored value',
      );
      expect(current.refreshes, hasLength(3));
      current.refreshes.last.complete([card('authoritative value', 6)]);
      await tester.pump();
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'authoritative value',
      );
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'restored UI inherits known native revision before a late reply',
    (tester) async {
      final storage = CountingStorage()
        ..data = {
          'theme': 'white',
          'glass': 'frosted',
          'background': 'ambient',
          'ideas': [card('restored current value', 0).toJson()],
        };
      final backend = DeferredWorkbench();
      backend.known['same-card'] = BigInt.from(9);
      await tester.pumpWidget(MorrowApp(storage: storage, workbench: backend));
      await tester.pumpAndSettle();
      final dynamic studio = tester.state(find.byType(Studio));
      final request =
          studio.pluginChange(PluginAction.edit, card('old intent', 7))
              as Future<Idea?>;
      backend.pending.single.complete(card('historical reply', 8));
      expect(await request, isNull);
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'restored current value',
      );
      backend.refreshes.single.complete([card('fresh read', 9)]);
      await tester.pump();
      expect(
        (studio.ideas as List<Idea>)
            .singleWhere((idea) => idea.id == 'same-card')
            .title,
        'fresh read',
      );
      await tester.pumpWidget(const SizedBox());
    },
  );
}
