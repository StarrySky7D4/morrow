// Real faulted Windows host followed by recovery through the visible controls.
import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart' show debugPrintSynchronously;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/mutation_recovery_manager.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'mutation_recovery_real_native_test.dart' show readCommand, reclaim;

Uint8List _token(int value) => Uint8List(32)..[0] = value;

Finder _control(String id) => find.byKey(ValueKey('mutation-recovery-$id'));

bool _enabled(WidgetTester tester, String id) =>
    _control(id).evaluate().isNotEmpty &&
    tester.widget<OutlinedButton>(_control(id)).onPressed != null;

Future<void> _tap(WidgetTester tester, String id) async {
  FocusManager.instance.primaryFocus?.unfocus();
  tester.testTextInput.hide();
  await tester.pump(const Duration(milliseconds: 100));
  await tester.ensureVisible(_control(id));
  await tester.pump(const Duration(milliseconds: 100));
  if (!_enabled(tester, id)) {
    await _awaitUi(
      tester,
      () => _enabled(tester, id),
      stage: '$id enabled immediately before tap',
    );
    await tester.ensureVisible(_control(id));
    await tester.pump(const Duration(milliseconds: 100));
  }
  expect(_enabled(tester, id), isTrue);
  await tester.tap(_control(id));
  await tester.pump(const Duration(milliseconds: 100));
  final error = tester.takeException();
  if (error != null) throw error;
}

Future<void> _awaitUi(
  WidgetTester tester,
  bool Function() ready, {
  required String stage,
  String Function()? diagnostics,
}) async {
  debugPrint('mutation crash widget: waiting for $stage');
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  var polls = 0;
  while (!ready()) {
    if (DateTime.now().isAfter(deadline)) {
      fail(
        'Real recovery UI did not reach $stage within 30 seconds: '
        '${diagnostics?.call() ?? ''}',
      );
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump(const Duration(milliseconds: 100));
    final error = tester.takeException();
    if (error != null) throw error;
    if (++polls % 10 == 0 && _enabled(tester, 'refresh')) {
      await _tap(tester, 'refresh');
    }
  }
  await tester.pump();
  debugPrint('mutation crash widget: reached $stage');
}

Future<T> _native<T>(WidgetTester tester, Future<T> Function() action) async {
  late T value;
  var done = false;
  Object? failure;
  StackTrace? stack;
  unawaited(
    Future.sync(action).then<void>(
      (result) {
        value = result;
        done = true;
      },
      onError: (Object error, StackTrace trace) {
        failure = error;
        stack = trace;
        done = true;
      },
    ),
  );
  await _awaitUi(tester, () => done, stage: 'native action completion');
  if (failure != null) Error.throwWithStackTrace(failure!, stack!);
  return value;
}

Widget _screen(
  RustWorkbench backend,
  PluginLibraryEntry plugin,
  BigInt revision,
) => MaterialApp(
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: MutationRecoveryManager(
        backend: backend.mutationTasks,
        ioBackend: backend,
        plugins: [plugin],
        registryRevision: revision,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(8),
      ),
    ),
  ),
);

void main() {
  LiveTestWidgetsFlutterBinding.ensureInitialized();
  final environment = Platform.environment;
  final executable = environment['MORROW_MUTATION_CRASH_HOST'];
  final builtin = environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = environment['MORROW_MUTATION_TASK_PACKAGE'];
  final kind = environment['MORROW_MUTATION_CRASH_KIND'];
  final point = environment['MORROW_MUTATION_CRASH_POINT'];
  final expectCrash = environment['MORROW_MUTATION_EXPECT_CRASH'] != '0';
  final faultVariable = kind == 'delete'
      ? 'MORROW_FILE_DELETE_FAULT'
      : 'MORROW_FILE_CREATE_FAULT';
  const subject = 'native.crash.widget.recovery';

  testWidgets(
    'real $kind $point expectCrash=$expectCrash recovers through widget controls',
    (tester) async {
      expect(kind, anyOf('create', 'delete'));
      expect(point, anyOf('after-claim', 'after-effect', 'after-observe'));
      expect(environment[faultVariable], point);
      if (!expectCrash) expect(kind, 'create');

      Directory? directory;
      RustWorkbench? backend;
      var crashedCurrent = false;
      late PluginLibraryEntry plugin;
      late BigInt revision;
      late File target;
      late Uint8List originalPlan;
      late bool beforeRecovery;
      final isCreate = kind == 'create';
      final disposition = isCreate
          ? MutationDisposition.create
          : MutationDisposition.delete;
      try {
        debugPrint('mutation crash widget: preparing real host');
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-mutation-crash-widget-',
          );
          Future<RustWorkbench> open() => RustWorkbench.open(
            executable: executable!,
            package: builtin!,
            directory: directory!,
            managed: true,
          );

          backend = await open();
          debugPrint('mutation crash widget: original host opened');
          final preview = await backend!.inspectPlugin(fixture!);
          final candidate = preview.entries.single;
          expect(
            candidate.declaredIo,
            contains(isCreate ? 'file-create' : 'file-delete'),
          );
          await backend!.importPlugin(
            fixture,
            candidate.digest,
            preview.revision,
          );
          var catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
          await backend!.configureExternalIo(plugin, catalog.revision, [
            isCreate ? 'file-create' : 'file-delete',
          ]);
          catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
          await backend!.configureExternal(plugin, catalog.revision, [], true);
          catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);

          final targets = Directory('${directory!.path}/targets')..createSync();
          final created = File('${targets.path}/created.bin');
          final deleted = File('${targets.path}/deleted.bin')
            ..writeAsBytesSync([4, 5, 6]);
          expect(deleted.readAsBytesSync(), [4, 5, 6]);
          target = isCreate ? created : deleted;
          final start = await backend!.mutationTasks.startSelected(
            MutationStartRequest(
              submission: _token(1),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              disposition: disposition,
              selectedPath: isCreate ? targets.path : deleted.path,
              relativePath: isCreate ? 'created.bin' : '',
              subject: subject,
              approvalSha256: _token(99),
              timeoutMs: 30000,
            ),
          );
          final key = start.io.key!;
          expect(
            (await readCommand(backend!, start)).kind,
            MutationResultKind.selected,
          );
          final planned = await readCommand(
            backend!,
            await backend!.mutationTasks.submitBuildPlan(
              key,
              _token(2),
              operationId: 'crash-widget-$kind-$point',
              contentLength: BigInt.zero,
              contentSha256: isCreate
                  ? Uint8List.fromList(sha256.convert([]).bytes)
                  : null,
            ),
          );
          originalPlan = planned.plan!;
          expect(originalPlan, isNotEmpty);
          expect(
            (await readCommand(
              backend!,
              await backend!.mutationTasks.submitPrepare(
                key,
                _token(3),
                originalPlan,
              ),
            )).phase,
            MutationPhase.prepared,
          );
          if (isCreate) {
            final committed = await readCommand(
              backend!,
              await backend!.mutationTasks.submitCommitContent(key, _token(4)),
            );
            expect(committed.durableContent, isTrue);
            expect(created.existsSync(), isFalse);
          }

          debugPrint('mutation crash widget: executing fault point');
          Object? executeError;
          MutationResult? executed;
          try {
            executed = await readCommand(
              backend!,
              await backend!.mutationTasks.submitExecute(key, _token(5)),
            );
          } catch (error) {
            executeError = error;
          }
          if (expectCrash) {
            expect(executeError, isNotNull);
            expect(executed, isNull);
            expect(await backend!.process.exitCode, 86);
            crashedCurrent = true;
          } else {
            expect(executeError, isNull);
            expect(executed!.kind, MutationResultKind.created);
            expect(executed.effect, MutationEffect.osSucceeded);
            expect(created.readAsBytesSync(), isEmpty);
            expect(
              (await readCommand(
                backend!,
                await backend!.mutationTasks.submitRelease(key, _token(6)),
              )).kind,
              MutationResultKind.released,
            );
            await reclaim(backend!, key);
          }
          debugPrint('mutation crash widget: execute outcome checked');

          final shouldExist = !expectCrash || point != 'after-claim';
          expect(target.existsSync(), isCreate ? shouldExist : !shouldExist);
          if (!isCreate && target.existsSync()) {
            expect(target.readAsBytesSync(), [4, 5, 6]);
          }
          if (crashedCurrent) {
            try {
              await backend!.close();
            } catch (_) {
              // This owner was already proven to have exited with code 86.
            }
          } else {
            await backend!.close();
          }
          backend = null;
          crashedCurrent = false;

          backend = await open();
          debugPrint('mutation crash widget: recovery host opened');
          catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
          revision = catalog.revision;
          beforeRecovery = target.existsSync();
        });
        debugPrint('mutation crash widget: host setup complete');

        final active = backend!;
        final session = active.mutationRecovery;
        final view = MutationRecoveryViewState.forSession(session);
        await tester.pumpWidget(_screen(active, plugin, revision));
        debugPrint('mutation crash widget: panel mounted');
        await _awaitUi(
          tester,
          () => session.canStart,
          stage: 'initial local state',
        );

        if (!isCreate) {
          debugPrint('mutation crash widget: choosing delete disposition');
          final dispositionField = find.byKey(
            const ValueKey('mutation-recovery-disposition'),
          );
          await tester.tap(dispositionField);
          await tester.pump(const Duration(milliseconds: 300));
          final label = L10n.forLocale(const Locale('en')).pluginsIoFileDelete;
          await tester.tap(find.text(label).last);
          await tester.pump(const Duration(milliseconds: 300));
        }
        final packageField = find.byKey(
          const ValueKey('mutation-recovery-package'),
        );
        debugPrint('mutation crash widget: choosing package');
        await tester.tap(packageField);
        await tester.pump(const Duration(milliseconds: 300));
        await tester.tap(find.text(plugin.name).last);
        await tester.pump(const Duration(milliseconds: 300));
        expect(
          tester.widget<DropdownButton<String>>(packageField).value,
          isNotNull,
        );
        await tester.enterText(
          find.byKey(const ValueKey('mutation-recovery-subject')),
          subject,
        );
        debugPrint('mutation crash widget: subject entered');
        await tester.pump(const Duration(milliseconds: 100));
        await _awaitUi(
          tester,
          () => _enabled(tester, 'discover'),
          stage: 'selected scope ready',
        );
        await _tap(tester, 'discover');
        await _awaitUi(
          tester,
          () => session.discoveryRequest != null,
          stage: 'discovery submitted by button',
        );
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          stage: 'discovery read',
          diagnostics: () =>
              'phase=${session.phase} notice=${session.notice} '
              'busy=${session.busy} storage=${session.snapshot?.storage} '
              'delivery=${session.state?.delivery} '
              'request=${session.discoveryRequest != null}',
        );
        expect(session.page, isNull); // No implicit read.
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => find
              .byKey(const ValueKey('mutation-recovery-plan-0'))
              .evaluate()
              .isNotEmpty,
          stage: 'historical plan',
        );
        expect(session.page!.done, isTrue);
        expect(session.page!.plans, hasLength(1));
        expect(session.page!.plans!.single, originalPlan);
        expect(session.page!.phase, MutationPhase.none);
        expect(session.page!.effect, MutationEffect.unspecified);
        final selected = find.byKey(const ValueKey('mutation-recovery-plan-0'));
        await tester.ensureVisible(selected);
        await tester.tap(selected);
        await tester.pump();
        expect(view.selectedPlan, originalPlan);
        expect(view.selectedRequest!.subject, subject);
        expect(view.selectedRequest!.disposition, disposition);
        expect(_enabled(tester, 'reconcile'), isFalse);

        // Leaving and returning retains the delivered page and selection;
        // the panel does not issue another discovery or execute command.
        final discoveryRequest = session.discoveryRequest;
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpWidget(_screen(active, plugin, revision));
        await tester.pump();
        expect(session.discoveryRequest, same(discoveryRequest));
        expect(view.selectedPlan, originalPlan);
        expect(target.existsSync(), beforeRecovery);

        await _awaitUi(
          tester,
          () => _enabled(tester, 'release'),
          stage: 'remounted release',
        );
        await _tap(tester, 'release');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          stage: 'release read',
        );
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'ack'),
          stage: 'discovery ack',
        );
        await _tap(tester, 'ack');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'reconcile'),
          stage: 'reconcile enabled',
        );
        expect(target.existsSync(), beforeRecovery);

        await _tap(tester, 'reconcile');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          stage: 'reconcile read',
        );
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => view.lastReconciliation != null,
          stage: 'reconciled result',
        );
        final result = view.lastReconciliation!;
        final observed = !expectCrash || point == 'after-observe';
        expect(
          result.phase,
          observed ? MutationPhase.observed : MutationPhase.outcomeUnknown,
        );
        expect(
          result.effect,
          observed ? MutationEffect.osSucceeded : MutationEffect.unspecified,
        );
        final l = L10n.forLocale(const Locale('en'));
        if (observed) {
          expect(find.text(l.pluginsMutationObserved), findsWidgets);
          expect(find.text(l.pluginsMutationSucceeded), findsWidgets);
        } else {
          expect(find.text(l.pluginsMutationUnknown), findsWidgets);
          expect(find.text(l.pluginsMutationSucceeded), findsNothing);
        }
        expect(target.existsSync(), beforeRecovery);
        await _awaitUi(
          tester,
          () => _enabled(tester, 'ack'),
          stage: 'reconcile ack',
        );
        await _tap(tester, 'ack');
        await _awaitUi(
          tester,
          () => session.canStart,
          stage: 'acknowledged local state',
        );
        expect(target.existsSync(), beforeRecovery);
      } catch (error, stack) {
        debugPrintSynchronously('mutation crash widget primary error: $error');
        debugPrintSynchronously('$stack');
        rethrow;
      } finally {
        debugPrintSynchronously('mutation crash widget: teardown started');
        await tester.pumpWidget(const SizedBox.shrink());
        debugPrintSynchronously('mutation crash widget: panel unmounted');
        try {
          if (backend != null) {
            if (crashedCurrent) {
              try {
                await _native(tester, () => backend!.close());
              } catch (_) {
                // Only the previously proven crashed owner may fail close.
              }
            } else {
              await _native(tester, () => backend!.close());
            }
          }
        } finally {
          debugPrintSynchronously('mutation crash widget: backend closed');
          if (directory != null) {
            await tester.runAsync(() => removeTestDirectory(directory!));
          }
        }
        debugPrintSynchronously('mutation crash widget: teardown complete');
      }
    },
    skip:
        !Platform.isWindows ||
        executable == null ||
        builtin == null ||
        fixture == null ||
        kind == null ||
        point == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
