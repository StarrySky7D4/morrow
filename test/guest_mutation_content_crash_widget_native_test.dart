// Real protected-content Commit interruption, recovered by visible read-only
// controls on a fresh ordinary host. This test never issues Execute.
import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart' show debugPrintSynchronously;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_session.dart';
import 'package:morrow_studio/plugins/mutation_recovery_manager.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'guest_mutation_crash_support.dart'
    show
        guestCrashSubject,
        reviewGuestCrash,
        saveGuestContentCrashPlan,
        verifyGuestContentCrashStore;

Finder _button(String id) => find.byKey(ValueKey('mutation-recovery-$id'));
void _noTargetOrTemp(File target) {
  expect(target.existsSync(), isFalse);
  expect(target.parent.listSync(followLinks: false), isEmpty);
}

bool _enabled(WidgetTester tester, String id) =>
    _button(id).evaluate().isNotEmpty &&
    tester.widget<OutlinedButton>(_button(id)).onPressed != null;

Future<void> _awaitUi(
  WidgetTester tester,
  bool Function() ready,
  String stage,
) async {
  final clock = Stopwatch()..start();
  var polls = 0;
  while (!ready() && clock.elapsed < const Duration(seconds: 45)) {
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
  expect(ready(), isTrue, reason: stage);
  await tester.pump();
}

Future<void> _tap(WidgetTester tester, String id) async {
  FocusManager.instance.primaryFocus?.unfocus();
  tester.testTextInput.hide();
  await tester.pump(const Duration(milliseconds: 100));
  await tester.ensureVisible(_button(id));
  await tester.pump(const Duration(milliseconds: 100));
  expect(_enabled(tester, id), isTrue, reason: '$id enabled');
  await tester.tap(_button(id));
  await tester.pump(const Duration(milliseconds: 100));
  final error = tester.takeException();
  if (error != null) throw error;
}

Future<T> _native<T>(WidgetTester tester, Future<T> Function() action) async {
  late T result;
  Object? failure;
  StackTrace? trace;
  var done = false;
  unawaited(
    Future.sync(action).then<void>(
      (value) {
        result = value;
        done = true;
      },
      onError: (Object error, StackTrace stack) {
        failure = error;
        trace = stack;
        done = true;
      },
    ),
  );
  await _awaitUi(tester, () => done, 'native operation completed');
  if (failure != null) Error.throwWithStackTrace(failure!, trace!);
  return result;
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
  final env = Platform.environment;
  final faultHost = env['MORROW_MUTATION_CRASH_HOST'];
  final normalHost = env['MORROW_WORKBENCH_HOST'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  final fixture = env['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = env['MORROW_GUEST_MUTATION_LANGUAGE'];
  final moduleHash = env['MORROW_GUEST_MUTATION_WASM_SHA256'];
  final packageHash = env['MORROW_GUEST_MUTATION_PACKAGE_SHA256'];
  final point = env['MORROW_TEST_CRASH_AT'];
  final expectCrash = env['MORROW_MUTATION_EXPECT_CRASH'] != '0';
  final contentBytes = int.tryParse(
    env['MORROW_GUEST_MUTATION_CONTENT_BYTES'] ?? '',
  );

  testWidgets(
    'real Rust 16MiB $point Prepare crash recovers as Prepared',
    (tester) async {
      expect(language, 'rust');
      expect(
        point,
        anyOf(<String>[
          'file-content-after-bytes',
          'file-content-after-receipt',
          'file-content-before-commit',
          'file-content-after-commit',
        ]),
      );
      expect(contentBytes, 16 * 1024 * 1024);
      expect(moduleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(packageHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      for (final path in [faultHost!, normalHost!, builtin!, fixture!]) {
        expect(File(path).existsSync(), isTrue, reason: path);
      }
      expect(
        sha256.convert(File(fixture).readAsBytesSync()).toString(),
        packageHash,
      );

      Directory? directory;
      RustWorkbench? backend;
      var crashedCurrent = false;
      late PluginLibraryEntry plugin;
      late BigInt revision;
      late File target;
      late Uint8List plan;
      late Uint8List originalContent;
      late String operationId;
      var storeVerifiable = false, businessPassed = false;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-guest-content-widget-',
          );
          backend = await RustWorkbench.open(
            executable: faultHost,
            package: builtin,
            directory: directory!,
            managed: true,
          );
          final prepared = await reviewGuestCrash(
            backend: backend!,
            guestPackagePath: fixture,
            directory: directory!,
            kind: 'create',
            point: point!,
            language: language!,
          );
          target = prepared.target;
          plan = Uint8List.fromList(prepared.plan);
          originalContent = prepared.content;
          operationId = prepared.operationId;
          expect(prepared.content.length, 16 * 1024 * 1024);
          expect(plan, isNotEmpty);
          await saveGuestContentCrashPlan(directory!, plan);
          _noTargetOrTemp(target);
          expect(prepared.session.canPrepare, isTrue);
          expect(prepared.session.canExecute, isFalse);
          expect(prepared.session.state?.permitDelivered, isFalse);
          expect(prepared.session.state?.effectAttempted, isFalse);
          debugPrintSynchronously(
            'GUEST_CONTENT_CRASH_WIDGET_BYTES=${prepared.content.length}',
          );
          debugPrintSynchronously('GUEST_CONTENT_CRASH_WIDGET_CHUNKS=274');

          Object? prepareError;
          try {
            await prepared.session.prepare(
              reviewedPlanSha256: prepared.planSha256,
            );
          } catch (error) {
            prepareError = error;
          }
          if (expectCrash) {
            expect(
              await backend!.process.exitCode.timeout(
                const Duration(seconds: 35),
              ),
              86,
            );
            crashedCurrent = true;
            expect(prepared.session.outcome, isNull);
            expect(prepared.session.canExecute, isFalse);
            expect(prepared.session.state?.permitDelivered, isFalse);
            expect(prepared.session.state?.effectAttempted, isFalse);
          } else {
            expect(prepareError, isNull);
            expect(prepared.session.error, isNull);
            expect(prepared.session.state?.durableContent, isTrue);
            expect(prepared.session.stagedBytes, BigInt.from(contentBytes!));
            // A completed Prepare is still not an Execute approval.
            expect(prepared.session.canExecute, isTrue);
            expect(prepared.session.state?.permitDelivered, isFalse);
            expect(prepared.session.state?.effectAttempted, isFalse);
            await prepared.session.release();
            final clock = Stopwatch()..start();
            while (!prepared.session.canAcknowledge &&
                clock.elapsed < const Duration(seconds: 25)) {
              await prepared.session.refresh();
              await Future<void>.delayed(const Duration(milliseconds: 10));
            }
            expect(prepared.session.canAcknowledge, isTrue);
            await prepared.session.acknowledge();
          }
          _noTargetOrTemp(target);

          final oldHost = backend!;
          if (crashedCurrent) {
            try {
              await oldHost.close();
            } catch (_) {
              // This host's exit code was proved to be 86.
            }
          } else {
            await oldHost.close();
          }
          expect(
            await oldHost.process.exitCode.timeout(const Duration(seconds: 5)),
            expectCrash ? 86 : 0,
          );
          backend = null;
          crashedCurrent = false;
          backend = await RustWorkbench.open(
            executable: normalHost,
            package: builtin,
            directory: directory!,
            managed: true,
          );
          storeVerifiable = true;
          final catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere(
            (entry) => entry.id == prepared.plugin.id,
          );
          expect(plugin.digest, prepared.plugin.digest);
          revision = catalog.revision;
        });

        final active = backend!;
        final session = active.mutationRecovery;
        final view = MutationRecoveryViewState.forSession(session);
        await tester.pumpWidget(_screen(active, plugin, revision));
        await _awaitUi(tester, () => session.canStart, 'local recovery');
        await tester.tap(_button('package'));
        await tester.pump(const Duration(milliseconds: 300));
        await tester.tap(find.text(plugin.name).last);
        await tester.pump(const Duration(milliseconds: 300));
        await _tap(tester, 'guest-scope');
        expect(
          tester.widget<TextField>(_button('subject')).controller!.text,
          guestCrashSubject,
        );
        await _awaitUi(
          tester,
          () => _enabled(tester, 'discover'),
          'scope ready',
        );
        await _tap(tester, 'discover');
        await _awaitUi(tester, () => _enabled(tester, 'read'), 'page ready');
        expect(session.page, isNull);
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => _button('plan-0').evaluate().isNotEmpty,
          'original plan discovered',
        );
        expect(session.page!.plans, hasLength(1));
        expect(session.page!.plans!.single, plan);
        await tester.ensureVisible(_button('plan-0'));
        await tester.tap(_button('plan-0'));
        await tester.pump();
        expect(view.selectedPlan, plan);
        expect(view.selectedRequest!.subject, guestCrashSubject);
        expect(view.selectedRequest!.disposition, MutationDisposition.create);
        _noTargetOrTemp(target);

        // Remounting or polling cannot resume Commit or Execute.
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpWidget(_screen(active, plugin, revision));
        await tester.pump();
        expect(view.selectedPlan, plan);
        _noTargetOrTemp(target);
        await _awaitUi(tester, () => _enabled(tester, 'release'), 'release');
        await _tap(tester, 'release');
        await _awaitUi(tester, () => _enabled(tester, 'read'), 'release read');
        await _tap(tester, 'read');
        await _awaitUi(tester, () => _enabled(tester, 'ack'), 'scan exit');
        await _tap(tester, 'ack');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'reconcile'),
          'reconcile',
        );
        _noTargetOrTemp(target);
        await _tap(tester, 'reconcile');
        await _awaitUi(tester, () => _enabled(tester, 'read'), 'history read');
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => view.lastReconciliation != null,
          'historical Prepared',
        );
        final result = view.lastReconciliation!;
        expect(result.operationId, operationId);
        expect(result.record, isNotEmpty);
        expect(result.phase, MutationPhase.prepared);
        expect(result.effect, MutationEffect.unspecified);
        expect(result.outcome, isNull);
        debugPrintSynchronously('GUEST_CONTENT_EXECUTE_CALLS=0');
        final l = L10n.forLocale(const Locale('en'));
        expect(find.text(l.pluginsMutationPrepared), findsWidgets);
        expect(find.text(l.pluginsMutationSucceeded), findsNothing);
        _noTargetOrTemp(target);
        await _awaitUi(tester, () => _enabled(tester, 'ack'), 'reconcile exit');
        await _tap(tester, 'ack');
        await _awaitUi(tester, () => session.canStart, 'reclaimed local');
        _noTargetOrTemp(target);
        final firstRecord = Uint8List.fromList(result.record!);
        final newGuest = GuestMutationExecutionSession.forBackend(
          active.guestMutationTasks,
          active,
        );
        await _native(tester, newGuest.refresh);
        expect(newGuest.canExecute, isFalse);

        // Start a second independent reconciliation through the visible
        // button. It must read the same protected original, without promoting
        // Prepared to Observed or granting an Execute permit.
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpWidget(_screen(active, plugin, revision));
        await tester.pump();
        expect(view.selectedPlan, plan);
        await _awaitUi(
          tester,
          () => _enabled(tester, 'reconcile'),
          'second reconcile',
        );
        await _tap(tester, 'reconcile');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          'second history read',
        );
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () =>
              view.lastReconciliation != null &&
              !identical(view.lastReconciliation, result),
          'second historical Prepared',
        );
        final repeated = view.lastReconciliation!;
        expect(repeated.operationId, operationId);
        expect(repeated.record, orderedEquals(firstRecord));
        expect(repeated.phase, MutationPhase.prepared);
        expect(repeated.effect, MutationEffect.unspecified);
        expect(repeated.outcome, isNull);
        _noTargetOrTemp(target);
        await _awaitUi(
          tester,
          () => _enabled(tester, 'ack'),
          'second reconcile exit',
        );
        await _tap(tester, 'ack');
        await _awaitUi(tester, () => session.canStart, 'second local state');
        _noTargetOrTemp(target);
        await _native(tester, newGuest.refresh);
        expect(newGuest.canExecute, isFalse);
        businessPassed = true;
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        final current = backend;
        var exited = current == null && !storeVerifiable;
        int? exitCode;
        Object? closeError;
        StackTrace? closeStack;
        if (current != null) {
          try {
            if (crashedCurrent) {
              try {
                await _native(tester, current.close);
              } catch (_) {
                // Only an already proven crashed host may fail close.
              }
            } else {
              await _native(tester, current.close);
            }
          } catch (error, stack) {
            closeError = error;
            closeStack = stack;
          }
          try {
            exitCode = await tester.runAsync<int?>(() async {
              try {
                return await current.process.exitCode.timeout(
                  const Duration(seconds: 5),
                );
              } catch (_) {
                return null;
              }
            });
            exited = exitCode != null;
          } catch (_) {
            // Unknown process lifetime retains the protected Store.
          }
        }
        Object? exitError;
        StackTrace? exitStack;
        if ((storeVerifiable || businessPassed) && (!exited || exitCode != 0)) {
          exitError = StateError(
            'Recovery host exit was not confirmed as code 0 '
            '(observed: $exitCode); protected Store retained',
          );
          exitStack = StackTrace.current;
        }
        Object? verificationError;
        StackTrace? verificationStack;
        var verified = false;
        if (exited && exitError == null && storeVerifiable) {
          try {
            final result = await tester.runAsync<bool>(() async {
              try {
                await verifyGuestContentCrashStore(
                  directory: directory!,
                  plan: plan,
                  content: originalContent,
                  point: point!,
                  expectCrash: expectCrash,
                );
                return true;
              } catch (error, stack) {
                verificationError = error;
                verificationStack = stack;
                return false;
              }
            });
            verified = result == true;
          } catch (error, stack) {
            verificationError = error;
            verificationStack = stack;
          }
          if (!verified && verificationError == null) {
            verificationError = StateError(
              'Independent protected Store verification did not complete',
            );
            verificationStack = StackTrace.current;
          }
        }
        Object? cleanupError;
        StackTrace? cleanupStack;
        var cleaned = false;
        if (directory != null) {
          if (verified && closeError == null && businessPassed) {
            try {
              final result = await tester.runAsync<bool>(() async {
                try {
                  await removeTestDirectory(directory!);
                  return true;
                } catch (error, stack) {
                  cleanupError = error;
                  cleanupStack = stack;
                  return false;
                }
              });
              cleaned = result == true;
            } catch (error, stack) {
              cleanupError = error;
              cleanupStack = stack;
            }
            if (!cleaned && cleanupError == null) {
              cleanupError = StateError(
                'Protected Store cleanup did not complete',
              );
              cleanupStack = StackTrace.current;
            }
          }
          if (!cleaned) {
            debugPrintSynchronously(
              'GUEST_CONTENT_CRASH_WIDGET_STORE_PRESERVED=${directory!.path}',
            );
          }
        }
        if (closeError != null) {
          Error.throwWithStackTrace(closeError, closeStack!);
        }
        if (exitError != null) {
          Error.throwWithStackTrace(exitError, exitStack!);
        }
        if (verificationError != null) {
          Error.throwWithStackTrace(verificationError!, verificationStack!);
        }
        if (cleanupError != null) {
          Error.throwWithStackTrace(cleanupError!, cleanupStack!);
        }
        if (businessPassed && verified && cleaned) {
          debugPrintSynchronously(
            'GUEST_CONTENT_CRASH_WIDGET_PASS=$point:${expectCrash ? 'crash' : 'normal'}',
          );
        } else if (businessPassed) {
          throw StateError('Guest content crash verification was incomplete');
        }
      }
    },
    skip:
        !Platform.isWindows ||
        faultHost == null ||
        normalHost == null ||
        builtin == null ||
        fixture == null ||
        point == null,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
