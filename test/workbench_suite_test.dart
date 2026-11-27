import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/schedule/schedule_store.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/ui/inspector_drawer.dart';
import 'package:morrow_studio/widgets_suite/automation_engine.dart';
import 'package:morrow_studio/widgets_suite/countdown_store.dart';
import 'package:morrow_studio/widgets_suite/detached_window_bridge.dart';
import 'package:morrow_studio/widgets_suite/footer_reminder_controller.dart';
import 'package:morrow_studio/widgets_suite/timer_controller.dart';

void main() {
  test('TimerController counts down, completes, and tracks card focus minutes', () async {
    final store = MemoryTimerStore();
    final timer = TimerController(store, nowProvider: () => DateTime(2026, 11, 26, 9, 0));
    addTearDown(timer.dispose);
    await timer.restore();

    var started = 0;
    var completed = 0;
    timer.onStarted = () => started++;
    timer.onCompleted = () => completed++;

    timer.selectMode(WorkbenchTimerMode.custom, customMinutes: 1);
    timer.bindCard('card-alpha', label: '核心方案');
    expect(timer.formattedTime, '01:00');

    timer.start();
    expect(started, 1);
    for (var i = 0; i < 60; i++) {
      timer.tickOneSecond();
    }
    expect(timer.running, isFalse);
    expect(completed, 1);
    expect(timer.completedSessionsToday, 1);
    expect(timer.focusMinutesToday, 1);
    expect(timer.focusMinutesForCard('card-alpha'), 1);

    // Verify stopwatch mode
    timer.selectMode(WorkbenchTimerMode.stopwatch);
    timer.start();
    timer.tickOneSecond();
    timer.tickOneSecond();
    expect(timer.formattedTime, '00:02');

    // Verify custom duration minutes+seconds & restore persistence
    timer.setCustomDuration(minutes: 12, seconds: 30);
    expect(timer.mode, WorkbenchTimerMode.custom);
    expect(timer.formattedTime, '12:30');
    await Future<void>.delayed(const Duration(milliseconds: 20));

    final restored = TimerController(store, nowProvider: () => DateTime(2026, 11, 26, 9, 0));
    addTearDown(restored.dispose);
    await restored.restore();
    expect(restored.mode, WorkbenchTimerMode.custom);
    expect(restored.formattedTime, '12:30');
    timer.pause();
  });

  test('CountdownPreferences sorts upcoming/pinned items and persists', () async {
    final now = DateTime(2026, 11, 26);
    final store = MemoryCountdownStore();
    final prefs = CountdownPreferences(store, nowProvider: () => now);
    addTearDown(prefs.dispose);
    await prefs.restore();

    final far = await prefs.add(
      title: '年度发布',
      targetDate: DateTime(2026, 12, 26),
      pinned: true,
      linkedCardId: 'card-1',
    );
    final near = await prefs.add(
      title: '设计评审',
      targetDate: DateTime(2026, 11, 29),
      pinned: true,
    );
    final unpinnedSoon = await prefs.add(
      title: '日常清理',
      targetDate: DateTime(2026, 11, 27),
      pinned: false,
    );

    expect(prefs.entries.map((e) => e.id).toList(), [
      near.id,
      far.id,
      unpinnedSoon.id,
    ]);
    expect(near.daysFrom(now), 3);
    expect(prefs.primaryForFooter(now)?.id, near.id);
  });

  test('AutomationPreferences triggers actions and records execution logs', () async {
    final auto = AutomationPreferences(MemoryAutomationStore());
    addTearDown(auto.dispose);
    await auto.restore();

    var played = 0;
    var paused = 0;
    FooterAlert? lastAlert;
    final runtime = AutomationRuntime(
      onPlayMusic: () => played++,
      onPauseMusic: () => paused++,
      onFooterReminder: (alert) => lastAlert = alert,
      onEnterFocusMode: () {},
      onExitFocusMode: () {},
      onStartTimer: () {},
    );

    auto.trigger(AutomationTrigger.timerStarted, runtime: runtime);
    expect(played, 1);

    auto.trigger(AutomationTrigger.timerCompleted, runtime: runtime);
    expect(lastAlert, isNotNull);
    expect(lastAlert!.badge, '定时完成');
    expect(paused, 0); // disabled by default

    await auto.toggleRule('builtin-timer-done-pause');
    auto.trigger(AutomationTrigger.timerCompleted, runtime: runtime);
    expect(paused, 1);
    expect(auto.logs, isNotEmpty);
  });

  testWidgets(
    'Timer, Countdown, Automation, Footer Alert, Floating Drag/Resize & Native Detach work end-to-end',
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1440, 900);
      addTearDown(tester.view.reset);

      final calls = <MethodCall>[];
      const channel = MethodChannel('morrow/detached_widgets');
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (call) async {
            calls.add(call);
            if (call.method == 'isSupported') return true;
            return null;
          });
      addTearDown(() {
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(channel, null);
      });

      final timer = TimerController(MemoryTimerStore());
      final schedule = SchedulePreferences(MemoryScheduleStore());
      final countdown = CountdownPreferences(MemoryCountdownStore());
      final automation = AutomationPreferences(MemoryAutomationStore());
      final reminders = FooterReminderController();
      final bridge = DetachedWindowBridge(channel: channel);
      addTearDown(timer.dispose);
      addTearDown(schedule.dispose);
      addTearDown(countdown.dispose);
      addTearDown(automation.dispose);
      addTearDown(reminders.dispose);
      addTearDown(bridge.dispose);

      await schedule.add(title: '项目周会', date: DateTime.now(), timeText: '16:00');
      await countdown.add(
        title: '里程碑发布',
        targetDate: DateTime.now().add(const Duration(days: 5)),
        pinned: true,
      );

      await tester.pumpWidget(
        MorrowApp(
          storage: MemoryStorage(),
          schedulePreferences: schedule,
          timerController: timer,
          countdownPreferences: countdown,
          automationPreferences: automation,
          footerReminders: reminders,
          detachedWindowBridge: bridge,
          initialLocale: const Locale('zh'),
        ),
      );
      await tester.pumpAndSettle();

      // 1. Verify Timer, Countdown, Automation rendered in Studio inspector
      expect(find.byKey(const ValueKey('timer-display')), findsOneWidget);
      expect(find.textContaining('里程碑发布'), findsWidgets);

      // 2. Pop out Timer as in-app floating window, drag and resize it
      await tester.tap(find.byKey(const ValueKey('widget-float-timer')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('floating-window-timer')), findsOneWidget);

      final beforeRect = tester.getRect(
        find.byKey(const ValueKey('floating-window-timer')),
      );
      await tester.drag(
        find.byKey(const ValueKey('floating-drag-timer')),
        const Offset(60, 40),
      );
      await tester.pumpAndSettle();
      final afterMoveRect = tester.getRect(
        find.byKey(const ValueKey('floating-window-timer')),
      );
      expect(afterMoveRect.left, greaterThan(beforeRect.left + 30));
      expect(afterMoveRect.top, greaterThan(beforeRect.top + 20));

      await tester.drag(
        find.byKey(const ValueKey('floating-resize-timer')),
        const Offset(50, 45),
      );
      await tester.pumpAndSettle();
      final afterResizeRect = tester.getRect(
        find.byKey(const ValueKey('floating-window-timer')),
      );
      expect(afterResizeRect.width, greaterThan(afterMoveRect.width + 25));
      expect(afterResizeRect.height, greaterThan(afterMoveRect.height + 25));

      // 3. Start timer via play button -> footer timer pill appears; pause cleanly
      await tester.tap(
        find.byKey(const ValueKey('timer-play-pause')).last,
      );
      await tester.pump();
      expect(timer.running, isTrue);
      expect(find.byKey(const ValueKey('footer-timer-pill')), findsOneWidget);
      await tester.tap(
        find.byKey(const ValueKey('timer-play-pause')).last,
      );
      await tester.pump();
      expect(timer.running, isFalse);

      // 4. Trigger automation rule test-run -> shows footer alert banner -> dismiss
      final remindRule = automation.rules.firstWhere(
        (r) => r.id == 'builtin-timer-done-remind',
      );
      await tester.ensureVisible(
        find.byKey(ValueKey('automation-run-${remindRule.id}')),
      );
      await tester.tap(find.byKey(ValueKey('automation-run-${remindRule.id}')));
      await tester.pump();
      expect(find.byKey(const ValueKey('footer-alert-banner')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('footer-alert-dismiss')));
      await tester.pump();
      expect(find.byKey(const ValueKey('footer-alert-banner')), findsNothing);

      // 5. Detach floating timer to native Win32 mini window via bridge, then dock back
      await tester.tap(find.byKey(const ValueKey('floating-detach-timer')));
      await tester.pumpAndSettle();
      expect(calls.any((c) => c.method == 'openOrUpdate'), isTrue);
      expect(find.textContaining('已脱离主窗体'), findsOneWidget);

      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
            channel.name,
            channel.codec.encodeMethodCall(
              const MethodCall('onWidgetAction', {'id': 'timer', 'action': 'dock'}),
            ),
            (_) {},
          );
      await tester.pumpAndSettle();
      expect(calls.any((c) => c.method == 'close'), isTrue);
      expect(find.textContaining('已脱离主窗体'), findsNothing);

      // 6. Custom timer UI duration (minutes + seconds)
      await Scrollable.ensureVisible(
        tester.element(find.byKey(const ValueKey('timer-mode-custom'))),
        alignment: 0.3,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('timer-mode-custom')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('timer-custom-minutes')),
        '12',
      );
      await tester.enterText(
        find.byKey(const ValueKey('timer-custom-seconds')),
        '30',
      );
      await tester.pumpAndSettle();
      await Scrollable.ensureVisible(
        tester.element(find.byKey(const ValueKey('timer-custom-apply'))),
        alignment: 0.5,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('timer-custom-apply')));
      await tester.pumpAndSettle();
      expect(timer.formattedTime, '12:30');
      expect(find.text('12:30'), findsWidgets);

      // 7. Dragging floating timer outside viewport auto-detaches with relX/relY
      await Scrollable.ensureVisible(
        tester.element(find.byKey(const ValueKey('widget-float-timer'))),
        alignment: 0.3,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('widget-float-timer')));
      await tester.pumpAndSettle();
      calls.clear();
      await tester.drag(
        find.byKey(const ValueKey('floating-drag-timer')),
        const Offset(-240, 0),
      );
      await tester.pumpAndSettle();
      expect(
        calls.any((c) => c.method == 'openOrUpdate' && (c.arguments as Map)['id'] == 'timer'),
        isTrue,
      );
      final openWidgetArgs = calls
          .lastWhere((c) => c.method == 'openOrUpdate')
          .arguments as Map;
      for (final key in const [
        'surfaceArgb',
        'inkArgb',
        'mutedArgb',
        'lineArgb',
        'edgeArgb',
        'onAccentArgb',
        'cornerRadius',
        'fontFamily',
      ]) {
        expect(openWidgetArgs.containsKey(key), isTrue);
      }
      expect(find.byKey(const ValueKey('floating-window-timer')), findsNothing);
      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
            channel.name,
            channel.codec.encodeMethodCall(
              const MethodCall('onWidgetAction', {'id': 'timer', 'action': 'dock'}),
            ),
            (_) {},
          );
      await tester.pumpAndSettle();

      // 8. NewIdeaDialog: drag, resize, detach to standalone window, dock back & save
      await tester.tap(find.byKey(const ValueKey('header-new-idea')));
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);

      final glassFinder = find.descendant(
        of: find.byKey(const ValueKey('studio-dialog')),
        matching: find.byKey(const ValueKey('studio-dialog-drag')),
      );
      final dialogBoxBefore = tester.getRect(
        find.byKey(const ValueKey('studio-dialog-resize')),
      );
      await tester.drag(glassFinder, const Offset(48, 32));
      await tester.pumpAndSettle();
      final dialogAfterMove = tester.getRect(
        find.byKey(const ValueKey('studio-dialog-resize')),
      );
      final headerAfterMove = tester.getRect(glassFinder);
      expect(dialogAfterMove.left, greaterThan(dialogBoxBefore.left + 20));
      expect(dialogAfterMove.top, greaterThan(dialogBoxBefore.top + 15));

      await tester.drag(
        find.byKey(const ValueKey('studio-dialog-resize')),
        const Offset(-60, -40),
      );
      await tester.pumpAndSettle();
      final dialogAfterResize = tester.getRect(
        find.byKey(const ValueKey('studio-dialog-resize')),
      );
      final headerAfterResize = tester.getRect(glassFinder);
      expect((headerAfterResize.left - headerAfterMove.left).abs(), lessThan(1.0));
      expect((headerAfterResize.top - headerAfterMove.top).abs(), lessThan(1.0));
      expect(dialogAfterResize.right, lessThan(dialogAfterMove.right - 20));

      await tester.enterText(
        find.byKey(const ValueKey('idea-title')),
        '脱离前草稿',
      );
      await tester.tap(find.byKey(const ValueKey('idea-detach-window')));
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsNothing);
      final openEditorCall = calls.lastWhere((c) => c.method == 'openEditor');
      final openEditorArgs = openEditorCall.arguments as Map;
      final detachedId = openEditorArgs['id'] as String;
      expect(openEditorArgs['title'], '脱离前草稿');
      expect(openEditorArgs['pinned'], isTrue);
      expect(openEditorArgs['previewMode'], isFalse);
      for (final key in const [
        'surfaceArgb',
        'inkArgb',
        'mutedArgb',
        'lineArgb',
        'edgeArgb',
        'onAccentArgb',
        'cornerRadius',
        'frostedOpacity',
        'isSolid',
        'fontFamily',
      ]) {
        expect(openEditorArgs.containsKey(key), isTrue);
      }

      // Dock back from standalone window -> reopens dialog with native edits
      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
            channel.name,
            channel.codec.encodeMethodCall(
              MethodCall('onEditorAction', {
                'id': detachedId,
                'action': 'dock',
                'title': '桌面窗口修改后',
                'description': '自由编辑的正文',
                'category': '进行中',
                'todos': '验证完成',
              }),
            ),
            (_) {},
          );
      await tester.pumpAndSettle();
      expect(find.byType(NewIdeaDialog), findsOneWidget);
      expect(find.text('桌面窗口修改后'), findsOneWidget);

      // Detach again & save directly from native standalone window
      await tester.tap(find.byKey(const ValueKey('idea-detach-window')));
      await tester.pumpAndSettle();
      final openEditorCall2 = calls.lastWhere((c) => c.method == 'openEditor');
      final detachedId2 = (openEditorCall2.arguments as Map)['id'] as String;
      await TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .handlePlatformMessage(
            channel.name,
            channel.codec.encodeMethodCall(
              MethodCall('onEditorAction', {
                'id': detachedId2,
                'action': 'save',
                'title': '脱离窗口新建卡片',
                'description': '成功保存到工作台',
                'category': '灵感',
                'todos': '',
              }),
            ),
            (_) {},
          );
      await tester.pumpAndSettle();
      expect(find.text('脱离窗口新建卡片'), findsWidgets);

      // 9. Opening the saved card details dialog also allows direct detach
      await tester.tap(find.text('脱离窗口新建卡片').first);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('card-details-detach-window')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('card-details-detach-window')));
      await tester.pumpAndSettle();
      final cardDetachCall = calls.lastWhere((c) => c.method == 'openEditor');
      expect((cardDetachCall.arguments as Map)['title'], '脱离窗口新建卡片');
      expect((cardDetachCall.arguments as Map)['pinned'], isTrue);
      expect((cardDetachCall.arguments as Map)['previewMode'], isTrue);

      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    },
  );

  testWidgets(
    'StudioDialog scrolls vertically when receiving mouse wheel pointer scroll events',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1280, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(
        AppearanceScope(
          palette: const Palette(StudioTheme.white, GlassMode.frosted),
          child: MaterialApp(
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            locale: const Locale('en'),
            home: Scaffold(
              body: StudioDialog(
                title: '编辑卡片',
                width: 600,
                draggable: true,
                resizable: true,
                minHeight: 320,
                maxHeight: 420,
                content: Column(
                  children: List<Widget>.generate(
                    24,
                    (i) => SizedBox(
                      height: 48,
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: Text('内容行 $i'),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final scrollable = find.descendant(
        of: find.byKey(const ValueKey('studio-dialog')),
        matching: find.byType(Scrollable),
      );
      expect(scrollable, findsOneWidget);

      final before = tester.state<ScrollableState>(scrollable).position.pixels;
      expect(before, 0.0);

      final center = tester.getCenter(scrollable);
      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: center,
          scrollDelta: const Offset(0, 120),
        ),
      );
      await tester.pumpAndSettle();

      final after = tester.state<ScrollableState>(scrollable).position.pixels;
      expect(after, greaterThan(0.0));
    },
  );

  testWidgets(
    'Studio inspector supports collapsible modules, hides duplicate pulse on overview, uses crisp switch, and hides scrollbars',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1440, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      final schedule = SchedulePreferences(MemoryScheduleStore());
      final timer = TimerController(MemoryTimerStore());
      final countdown = CountdownPreferences(MemoryCountdownStore());
      final automation = AutomationPreferences(MemoryAutomationStore());
      final reminders = FooterReminderController();
      addTearDown(() {
        schedule.dispose();
        timer.dispose();
        countdown.dispose();
        automation.dispose();
        reminders.dispose();
      });
      await schedule.restore();
      await timer.restore();
      await countdown.restore();
      await automation.restore();

      await tester.pumpWidget(
        MorrowApp(
          storage: MemoryStorage(),
          schedulePreferences: schedule,
          timerController: timer,
          countdownPreferences: countdown,
          automationPreferences: automation,
          footerReminders: reminders,
          initialLocale: const Locale('zh'),
        ),
      );
      await tester.pumpAndSettle();

      // 1. No right-side Scrollbar in WorkbenchInspector.
      expect(
        find.descendant(
          of: find.byType(WorkbenchInspector),
          matching: find.byType(Scrollbar),
        ),
        findsNothing,
      );

      // 2. Duplicate Workspace Pulse is hidden on Overview, shown on Inbox.
      expect(find.text('空间脉搏'), findsNothing);
      await tester.tap(find.byKey(ValueKey('nav-${WorkbenchPage.inbox.id}')));
      await tester.pumpAndSettle();
      expect(find.text('空间脉搏'), findsOneWidget);

      // 3. Module chip toggles timer visibility and survives tab switch via PageStorage.
      expect(find.byKey(const ValueKey('timer-display')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('inspector-module-toggle-timer')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('timer-display')), findsNothing);

      await tester.tap(find.byKey(const ValueKey('inspector-tab-music')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('inspector-tab-studio')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('timer-display')), findsNothing);

      await tester.tap(find.byKey(const ValueKey('inspector-module-toggle-timer')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('timer-display')), findsOneWidget);

      // 4. Compact switch toggles built-in automation rule state.
      final firstRule = automation.rules.first;
      final wasEnabled = firstRule.enabled;
      final toggleFinder = find.byKey(ValueKey('automation-toggle-${firstRule.id}'));
      await Scrollable.ensureVisible(
        tester.element(toggleFinder),
        alignment: 0.5,
      );
      await tester.pumpAndSettle();
      await tester.tap(toggleFinder);
      await tester.pumpAndSettle();
      expect(
        automation.rules.firstWhere((r) => r.id == firstRule.id).enabled,
        !wasEnabled,
      );

      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    },
  );
}
