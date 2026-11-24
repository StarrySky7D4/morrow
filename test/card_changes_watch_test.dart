import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_studio/plugins/card_changes_watch.dart';
import 'package:morrow_studio/plugins/changes_watch_controls.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';

final class Observer implements ChangesObservationBackend {
  final selections = <ChangesWindowReview>[];
  Future<ReviewedChangesProgress?> Function() read = () async =>
      const ReviewedChangesProgress(
        ChangesBookmarkState.notSupplied,
        null,
        false,
      );
  @override
  Future<ReviewedChangesProgress?> observeChangesCheckpoint(
    String packageId,
    Uint8List digest,
    BigInt revision,
    ChangesWindowReview selection,
  ) {
    selections.add(selection);
    return read();
  }
}

final digest = Uint8List.fromList(List.filled(32, 21));
ChangesWindowReview selection(String card) =>
    ChangesWindowReview(cards: [card], pageSize: 1);

void main() {
  testWidgets(
    'the reviewed watch survives dismissal, reports busy and can be stopped after reopening',
    (tester) async {
      final backend = Observer();
      final watch = CardChangesWatch.forBackend(backend);
      addTearDown(watch.stop);
      Widget panel() => MaterialApp(
        home: Scaffold(
          body: ChangesWatchControls(
            backend: backend,
            packageId: 'example.cards',
            digest: digest,
            revision: BigInt.one,
            enabled: true,
            readSelection: () => selection('selected-card'),
          ),
        ),
      );
      await tester.pumpWidget(panel());
      await tester.tap(find.text('Watch these cards…'));
      await tester.pumpAndSettle();
      expect(find.textContaining('selected-card'), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(backend.selections, isEmpty);
      await tester.tap(find.text('Watch these cards…'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Start watching'));
      await tester.pumpAndSettle();
      expect(watch.active, isTrue);
      expect(backend.selections, hasLength(1));
      await tester.pumpWidget(const SizedBox.shrink());
      backend.read = () async => null;
      await tester.pump(CardChangesWatch.interval);
      await tester.pump();
      await tester.pumpWidget(panel());
      expect(find.text('Watching selected-card'), findsOneWidget);
      expect(find.text('Waiting for the original library.'), findsOneWidget);
      backend.read = () async => const ReviewedChangesProgress(
        ChangesBookmarkState.notSupplied,
        null,
        true,
      );
      await tester.pump(CardChangesWatch.interval);
      await tester.pump();
      expect(
        find.text('Changes are available for this selection.'),
        findsOneWidget,
      );
      await tester.tap(find.text('Stop watching'));
      await tester.pump();
      final calls = backend.selections.length;
      await tester.pump(const Duration(minutes: 3));
      expect(backend.selections, hasLength(calls));
      expect(watch.active, isFalse);
    },
  );

  test(
    'late results from a closed library cannot update a new observation',
    () async {
      final backend = Observer();
      final watch = CardChangesWatch.forBackend(backend);
      addTearDown(watch.stop);
      final old = Completer<ReviewedChangesProgress?>();
      backend.read = () => old.future;
      final first = watch.start(
        'old',
        digest,
        BigInt.one,
        selection('old-card'),
      );
      await Future<void>.delayed(Duration.zero);
      CardChangesWatch.closeFor(backend);
      backend.read = () async => const ReviewedChangesProgress(
        ChangesBookmarkState.notSupplied,
        null,
        true,
      );
      await watch.start('new', digest, BigInt.two, selection('new-card'));
      old.completeError(StateError('old library closed'));
      await first;
      expect(watch.active, isTrue);
      expect(watch.matches('new', digest, BigInt.two), isTrue);
      expect(watch.progress?.pending, isTrue);
      expect(watch.failure, isNull);
      backend.read = () async => throw StateError('selection revoked');
      await watch.check();
      expect(watch.active, isFalse);
      expect(watch.progress, isNull);
      expect(watch.failure, isA<StateError>());
    },
  );

  testWidgets('the original one-hour deadline ends an outstanding read', (
    tester,
  ) async {
    final backend = Observer();
    final watch = CardChangesWatch.forBackend(backend);
    final outstanding = Completer<ReviewedChangesProgress?>();
    backend.read = () => outstanding.future;
    final reading = watch.start(
      'example.cards',
      digest,
      BigInt.one,
      selection('selected'),
    );
    await tester.pump();
    await tester.pump(CardChangesWatch.lifetime);
    expect(watch.expired, isTrue);
    expect(watch.active, isFalse);
    outstanding.complete(
      const ReviewedChangesProgress(
        ChangesBookmarkState.notSupplied,
        null,
        true,
      ),
    );
    await reading;
    expect(watch.progress, isNull);
    expect(backend.selections, hasLength(1));
  });
}
