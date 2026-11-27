import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/schedule/schedule_store.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/ui/inspector_drawer.dart';
import 'package:morrow_studio/ui/schedule_calendar_panel.dart';

class _FailingScheduleStore implements ScheduleStore {
  String? stored;
  bool failWrites = false;

  @override
  Future<String?> read() async => stored;

  @override
  Future<void> write(String value) async {
    if (failWrites) throw StateError('disk full');
    stored = value;
  }
}

void main() {
  test('ScheduleEntry validates dates, times, and titles strictly', () {
    expect(ScheduleEntry.tryParseDateKey('2026-02-28'), DateTime(2026, 2, 28));
    expect(ScheduleEntry.tryParseDateKey('2026-02-30'), isNull);
    expect(ScheduleEntry.tryParseDateKey('2026-13-01'), isNull);
    expect(ScheduleEntry.isValidTimeText(''), isTrue);
    expect(ScheduleEntry.isValidTimeText('09:30'), isTrue);
    expect(ScheduleEntry.isValidTimeText('23:59'), isTrue);
    expect(ScheduleEntry.isValidTimeText('24:00'), isFalse);
    expect(ScheduleEntry.isValidTimeText('9:30'), isFalse);

    expect(
      () => ScheduleEntry(
        id: ScheduleEntry.nextId(),
        title: '   ',
        dateKey: '2026-11-26',
        createdAtMs: 1,
      ).validated(),
      throwsFormatException,
    );
  });

  test('SchedulePreferences sorts, persists, and rolls back on write failure', () async {
    final store = _FailingScheduleStore();
    final prefs = SchedulePreferences(store);
    addTearDown(prefs.dispose);
    await prefs.restore();

    final e1 = await prefs.add(
      title: '下午评审',
      date: DateTime(2026, 11, 26),
      timeText: '14:30',
      linkedCardId: 'card-1',
    );
    final e2 = await prefs.add(
      title: '全天准备',
      date: DateTime(2026, 11, 26),
    );
    final e3 = await prefs.add(
      title: '早会',
      date: DateTime(2026, 11, 26),
      timeText: '09:00',
    );
    expect(prefs.entries.map((e) => e.id).toList(), [e3.id, e1.id, e2.id]);
    expect(prefs.entriesForCard('card-1').single.title, '下午评审');
    expect(prefs.pendingCountForDate(DateTime(2026, 11, 26)), 3);

    // Reload from JSON
    final restored = SchedulePreferences(store);
    addTearDown(restored.dispose);
    await restored.restore();
    expect(restored.entries.length, 3);
    expect(restored.entriesForCard('card-1').single.timeText, '14:30');

    // Rollback on write failure
    store.failWrites = true;
    await expectLater(restored.toggleCompleted(e1.id), throwsStateError);
    expect(restored.entriesForCard('card-1').single.completed, isFalse);
  });

  testWidgets(
    'Calendar schedule widget creates, links, navigates, toggles, and deletes card associations',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.reset);

      final store = MemoryScheduleStore();
      final schedulePrefs = SchedulePreferences(store);
      addTearDown(schedulePrefs.dispose);
      await schedulePrefs.restore();

      await tester.pumpWidget(
        MorrowApp(
          storage: MemoryStorage(),
          schedulePreferences: schedulePrefs,
          initialLocale: const Locale('zh'),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(WorkbenchScheduleCard), findsOneWidget);
      expect(find.byKey(const ValueKey('schedule-month-label')), findsOneWidget);

      // Verify month navigation < and >
      final beforeMonth = tester
          .widget<Text>(find.byKey(const ValueKey('schedule-month-label')))
          .data!;
      await tester.tap(find.byKey(const ValueKey('schedule-next-month')));
      await tester.pumpAndSettle();
      final nextMonth = tester
          .widget<Text>(find.byKey(const ValueKey('schedule-month-label')))
          .data!;
      expect(nextMonth, isNot(beforeMonth));
      await tester.tap(find.byKey(const ValueKey('schedule-prev-month')));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<Text>(find.byKey(const ValueKey('schedule-month-label')))
            .data,
        beforeMonth,
      );

      // Create a new schedule entry linked to the first card
      await tester.tap(find.byKey(const ValueKey('schedule-add-button')));
      await tester.pumpAndSettle();

      await tester.enterText(
        find.byKey(const ValueKey('schedule-title-input')),
        '整理方案初稿',
      );
      await tester.enterText(
        find.byKey(const ValueKey('schedule-time-input')),
        '15:00',
      );
      await tester.tap(find.byKey(const ValueKey('schedule-card-picker')));
      await tester.pumpAndSettle();
      await tester.tap(find.textContaining('给灵感一个容器').last);
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('schedule-save-button')));
      await tester.pumpAndSettle();

      expect(schedulePrefs.entries.length, 1);
      final created = schedulePrefs.entries.single;
      expect(created.title, '整理方案初稿');
      expect(created.timeText, '15:00');
      expect(created.linkedCardId, isNotNull);

      final cardId = created.linkedCardId!;
      expect(
        find.byKey(ValueKey('schedule-item-${created.id}')),
        findsOneWidget,
      );
      expect(
        find.byKey(ValueKey('schedule-linked-card-${created.id}')),
        findsOneWidget,
      );
      expect(
        find.byKey(ValueKey('card-schedule-badge-$cardId')),
        findsOneWidget,
      );

      // Click the schedule's linked card chip to open its detail dialog
      await tester.ensureVisible(
        find.byKey(ValueKey('schedule-linked-card-${created.id}')),
      );
      await tester.tap(
        find.byKey(ValueKey('schedule-linked-card-${created.id}')),
      );
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('studio-dialog')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('idea-detail-add-schedule')),
        findsOneWidget,
      );

      // Add a second linked event directly from the card detail dialog
      await tester.ensureVisible(
        find.byKey(const ValueKey('idea-detail-add-schedule')),
      );
      await tester.tap(find.byKey(const ValueKey('idea-detail-add-schedule')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('schedule-title-input')),
        '复核细节',
      );
      await tester.tap(find.byKey(const ValueKey('schedule-save-button')));
      await tester.pumpAndSettle();
      expect(schedulePrefs.entriesForCard(cardId).length, 2);

      // Close card detail dialog
      await tester.tap(
        find.descendant(
          of: find.byKey(const ValueKey('studio-dialog')),
          matching: find.byIcon(Icons.close_rounded),
        ),
      );
      await tester.pumpAndSettle();

      // Badge should reflect 2 linked items ("+1")
      final badgeFinder = find.byKey(ValueKey('card-schedule-badge-$cardId'));
      expect(badgeFinder, findsOneWidget);
      expect(
        find.descendant(of: badgeFinder, matching: find.textContaining('+1')),
        findsOneWidget,
      );

      // Toggle and delete both entries cleanly
      for (final entry in List<ScheduleEntry>.from(schedulePrefs.entries)) {
        await tester.ensureVisible(
          find.byKey(ValueKey('schedule-toggle-${entry.id}')),
        );
        await tester.tap(find.byKey(ValueKey('schedule-toggle-${entry.id}')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(ValueKey('schedule-delete-${entry.id}')));
        await tester.pumpAndSettle();
      }
      expect(schedulePrefs.entries, isEmpty);
      expect(
        find.byKey(ValueKey('card-schedule-badge-$cardId')),
        findsNothing,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('Calendar & Schedule renders without overflow across viewports', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    for (final size in const [
      Size(1440, 900),
      Size(840, 800),
      Size(390, 844),
    ]) {
      tester.view.physicalSize = size;
      await tester.pumpWidget(
        MorrowApp(
          storage: MemoryStorage(),
          initialLocale: const Locale('zh'),
        ),
      );
      await tester.pumpAndSettle();

      if (size.width >= 1050) {
        expect(find.byType(WorkbenchScheduleCard), findsOneWidget);
      } else {
        // Open compact settings sheet and verify calendar is accessible without overflow
        await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
        await tester.pumpAndSettle();
        expect(find.byType(WorkbenchInspector), findsOneWidget);
      }
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    }
  });
}
