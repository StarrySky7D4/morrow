// Native Windows app + original Rust owner. Framework input injection and
// render captures do not certify OS mouse/keyboard or system clipboard input.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_manager.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:window_manager/window_manager.dart';

import '../test/service_run_real_native_fixture.dart';
import 'live_ui_helpers.dart';

Finder keyed(String key) => find.byKey(ValueKey(key));

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'service coexists with native workbench and compact settings',
    (tester) async {
      // Required integration qualification must fail rather than silently skip.
      expect(
        RealServiceFixture.available,
        isTrue,
        reason: 'Set the four MORROW_* native fixture paths',
      );
      final outputPath = Platform.environment['MORROW_WINDOW_TEST_OUTPUT'];
      expect(
        outputPath,
        isNotNull,
        reason: 'Use an isolated absolute output directory',
      );
      final output = Directory(outputPath!).absolute;
      final fixture = await RealServiceFixture.open();
      final root = GlobalKey();
      final en = L10n.forLocale(const Locale('en'));
      final stages = <String>[];
      try {
        await fixture.backend.saveUiLocale('en');
        final storage = await RustStudioStorage.open(fixture.backend);
        await initializeDesktopFrame();
        await windowManager.setSize(const Size(800, 820));
        await tester.pumpWidget(
          RepaintBoundary(
            key: root,
            child: MorrowApp(
              storage: storage,
              workbench: fixture.backend,
              initialLocale: const Locale('en'),
              nativeBackground: DesktopBackground(),
            ),
          ),
        );
        await waitForUi(
          tester,
          () => keyed('appearance-toggle').evaluate().isNotEmpty,
          reason: 'workbench initial load',
        );
        expect(keyed('compact-settings-back'), findsNothing);
        await tapVisible(tester, keyed('appearance-toggle'));
        await waitForUi(
          tester,
          () => find.byType(ServiceRunManager).evaluate().length == 1,
          reason: 'full application service panel',
        );
        final selection = find.descendant(
          of: find.byType(ServiceRunManager),
          matching: find.byType(DropdownButtonFormField<String>),
        );
        await waitForUi(
          tester,
          () =>
              tester
                  .widget<DropdownButton<String>>(
                    find.descendant(
                      of: selection,
                      matching: find.byType(DropdownButton<String>),
                    ),
                  )
                  .items
                  ?.length ==
              1,
          reason: 'approved finite service choice',
        );
        await tapVisible(tester, selection);
        await tapVisible(
          tester,
          find.textContaining('${fixture.plugin.name} ·').last,
        );
        await tester.ensureVisible(keyed('service-run-lifetime'));
        await tester.enterText(keyed('service-run-lifetime'), '120000');
        await tapVisible(tester, keyed('service-run-start'));
        await waitForUi(
          tester,
          () => fixture.session.service?.phase == ServiceRunPhase.running,
          reason: 'real listener running',
        );
        final task = fixture.session.task!.key!.toList();
        final submission = fixture.session.service!.submission.toList();
        expect(await fixture.post(), contains('executed-before'));
        await tester.ensureVisible(keyed('service-run-identity'));
        await saveBoundaryPng(
          tester,
          root,
          '${output.path}/01-service-running.png',
        );
        stages.add('UI start and authenticated real HTTP');

        // Compact page is dismissed using the production return button. Service
        // session must survive the settings subtree unmount without a new start.
        await tapVisible(tester, keyed('compact-settings-back'));
        await waitForUi(
          tester,
          () => keyed('compact-settings-back').evaluate().isEmpty,
          reason: 'return to content',
        );
        await tapVisible(tester, find.text(en.mainNewIdea));
        await waitForUi(
          tester,
          () => keyed('idea-title').evaluate().isNotEmpty,
          reason: 'real Rust editor session',
        );
        final fragment = await fixture.backend.studio.capture(
          'plain',
          'A\tB\n1\t2',
        );
        expect(fragment.markdown, contains('| A | B |'));
        const title = 'Native service coexistence';
        final description = '# During service\n\n${fragment.markdown}';
        await tester.enterText(keyed('idea-title'), title);
        await tester.enterText(keyed('idea-description'), description);
        await tapVisible(tester, keyed('idea-preview-toggle'));
        expect(keyed('idea-markdown-preview'), findsOneWidget);
        await saveBoundaryPng(
          tester,
          root,
          '${output.path}/02-editor-during-service.png',
        );
        await tapVisible(tester, keyed('idea-save'));
        await waitForUi(
          tester,
          () => find.byType(NewIdeaDialog).evaluate().isEmpty,
          reason: 'editor commit completed',
        );
        final card = (await fixture.backend.load()).singleWhere(
          (i) => i.title == title,
        );
        expect(card.description, description);
        stages.add('native capture conversion and UI content commit');

        await tapVisible(tester, keyed('appearance-toggle'));
        await tapVisible(tester, keyed('language-picker'));
        await tapVisible(tester, find.text(en.mainLanguageChinese).last);
        await waitForUi(
          tester,
          () => storage.read()['uiLocale'] == 'zh',
          reason: 'language persisted while service owns store',
        );
        expect(await fixture.backend.readUiLocale(), 'zh');
        await waitForUi(
          tester,
          () => fixture.session.trusted && !fixture.session.busy,
          reason: 'settings remount observation',
        );
        expect(fixture.session.task!.key, task);
        expect(fixture.session.service!.submission, submission);
        expect(fixture.session.service!.phase, ServiceRunPhase.running);
        expect(await fixture.post(second: true), contains('executed-after'));
        stages.add(
          'settings remount, locale commit, original task and second HTTP',
        );

        await tapVisible(tester, keyed('compact-settings-back'));
        await windowManager.setSize(const Size(1280, 900));
        await waitForUi(
          tester,
          () => keyed('settings-side-panel').evaluate().isNotEmpty,
          reason: 'desktop layout after resize',
        );
        await waitForUi(
          tester,
          () =>
              keyed('query-loading').evaluate().isEmpty &&
              find.text(title).evaluate().isNotEmpty,
          reason: 'native workspace query completes after resize',
        );
        expect(keyed('query-error'), findsNothing);
        await saveBoundaryPng(
          tester,
          root,
          '${output.path}/03-wide-workbench.png',
        );
        await tapVisible(tester, keyed('service-run-stop'));
        await waitForUi(
          tester,
          () => fixture.session.canAcknowledge,
          reason: 'actual listener exit and owner reclaim',
        );
        expect(fixture.session.task!.storage, IoStoragePhase.reclaimed);
        expect(fixture.session.task!.exit!.maintenance, IoJobError.none);
        await tester.ensureVisible(keyed('service-run-exit'));
        await saveBoundaryPng(tester, root, '${output.path}/04-reclaimed.png');
        await tapVisible(tester, keyed('service-run-acknowledge'));
        await waitForUi(
          tester,
          () => fixture.session.task?.storage == IoStoragePhase.local,
          reason: 'explicit acknowledgement',
        );
        stages.add('UI stop, actual reclaim and explicit acknowledgement');

        await tester.pumpWidget(const SizedBox());
        await fixture.backend.close();
        final reopened = await RustWorkbench.open(
          executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
          package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
          directory: fixture.directory,
          managed: true,
        );
        try {
          expect(await reopened.readUiLocale(), 'zh');
          final persisted = (await reopened.load()).singleWhere(
            (i) => i.id == card.id,
          );
          expect(persisted.description, description);
          expect(persisted.title, title);
        } finally {
          await reopened.close();
        }
        stages.add('same original store reopened with content and language');
        await output.create(recursive: true);
        await File('${output.path}/result.json').writeAsString(
          const JsonEncoder.withIndent('  ').convert({
            'passed': true,
            'stages': stages,
            'input':
                'Flutter framework injection in Windows integration runner',
            'capture': 'Flutter render boundary, not OS screenshot',
            'clipboard': 'conversion input fixture, not system clipboard',
          }),
        );
      } finally {
        await tester.pumpWidget(const SizedBox());
        await fixture.close(observeBeforeClose: false);
      }
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
