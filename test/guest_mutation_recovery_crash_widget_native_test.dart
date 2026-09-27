// A real Rust SDK guest crashes at a native file-effect boundary. A fresh,
// ordinary host recovers its original plan only through visible UI controls.
import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart' show debugPrintSynchronously;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/mutation_recovery_manager.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'guest_mutation_crash_support.dart'
    show guestCrashEffectHappened, guestCrashSubject, prepareGuestCrash;

Finder _control(String id) => find.byKey(ValueKey('mutation-recovery-$id'));

bool _enabled(WidgetTester tester, String id) =>
    _control(id).evaluate().isNotEmpty &&
    tester.widget<OutlinedButton>(_control(id)).onPressed != null;

Future<void> _awaitUi(
  WidgetTester tester,
  bool Function() ready, {
  required String stage,
}) async {
  final clock = Stopwatch()..start();
  var polls = 0;
  while (!ready() && clock.elapsed < const Duration(seconds: 40)) {
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
  await tester.ensureVisible(_control(id));
  await tester.pump(const Duration(milliseconds: 100));
  expect(_enabled(tester, id), isTrue, reason: '$id enabled');
  await tester.tap(_control(id));
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
  await _awaitUi(tester, () => done, stage: 'native operation completed');
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
  final wasmHash = env['MORROW_GUEST_MUTATION_WASM_SHA256'];
  final packageHash = env['MORROW_GUEST_MUTATION_PACKAGE_SHA256'];
  final kind = env['MORROW_MUTATION_CRASH_KIND'];
  final point = env['MORROW_MUTATION_CRASH_POINT'];
  final requestedContentBytes = int.tryParse(
    env['MORROW_GUEST_MUTATION_CONTENT_BYTES'] ?? '184393',
  );
  final expectCrash = env['MORROW_MUTATION_EXPECT_CRASH'] != '0';
  final faultVariable = kind == 'delete'
      ? 'MORROW_FILE_DELETE_FAULT'
      : 'MORROW_FILE_CREATE_FAULT';

  testWidgets(
    'real Rust guest $kind $point recovers through visible widget controls',
    (tester) async {
      expect(language, 'rust');
      expect(kind, anyOf('create', 'delete'));
      expect(
        point,
        kind == 'create'
            ? anyOf(<String>[
                'after-claim',
                'after-temp',
                'after-write-chunk',
                'after-write',
                'after-flush',
                'after-publish',
                'after-effect',
                'after-observe',
              ])
            : anyOf('after-claim', 'after-effect', 'after-observe'),
      );
      if (!expectCrash) {
        expect(point, 'after-claim');
      }
      if (kind == 'create') {
        expect(requestedContentBytes, inInclusiveRange(1, 16 * 1024 * 1024));
      }
      expect(env[faultVariable], point);
      expect(wasmHash, matches(RegExp(r'^[0-9a-f]{64}$')));
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
      late Uint8List preserved;
      late String subject;
      late String operationId;
      final isCreate = kind == 'create';
      final effected = guestCrashEffectHappened(kind!, point!, expectCrash);
      final deferSentinel = isCreate && !effected;
      final disposition = isCreate
          ? MutationDisposition.create
          : MutationDisposition.delete;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-guest-crash-widget-',
          );
          backend = await RustWorkbench.open(
            executable: faultHost,
            package: builtin,
            directory: directory!,
            managed: true,
          );
          final prepared = await prepareGuestCrash(
            backend: backend!,
            guestPackagePath: fixture,
            directory: directory!,
            kind: kind,
            point: point,
            language: language!,
          );
          target = prepared.target;
          plan = Uint8List.fromList(prepared.plan);
          subject = guestCrashSubject;
          operationId = prepared.operationId;
          expect(plan, isNotEmpty);
          expect(prepared.content.length, isCreate ? requestedContentBytes : 3);
          debugPrintSynchronously(
            'GUEST_CRASH_WIDGET_CONTENT_BYTES=${prepared.content.length}',
          );
          if (isCreate) {
            final chunks =
                (prepared.content.length +
                    GuestMutationValidation.maxChunkBytes -
                    1) ~/
                GuestMutationValidation.maxChunkBytes;
            if (requestedContentBytes == 16 * 1024 * 1024) {
              expect(chunks, 274);
            }
            debugPrintSynchronously('GUEST_CRASH_WIDGET_CHUNKS=$chunks');
          } else {
            debugPrintSynchronously('GUEST_CRASH_WIDGET_CHUNKS=0');
          }
          expect(target.existsSync(), !isCreate);

          Object? executeError;
          try {
            await prepared.session.execute(
              reviewedPlanSha256: prepared.planSha256,
            );
          } catch (error) {
            executeError = error;
          }
          if (expectCrash) {
            expect(await backend!.process.exitCode, 86);
            crashedCurrent = true;
            expect(prepared.session.outcome, isNull);
          } else {
            expect(executeError, isNull);
            expect(
              prepared.session.outcome?.phase,
              GuestMutationFramePhase.observed,
            );
            expect(
              prepared.session.outcome?.effect,
              GuestMutationFrameEffect.osSucceeded,
            );
          }

          // Inspect the real OS effect before replacing the target. Recovery
          // must preserve these unrelated bytes rather than replaying Execute.
          expect(target.existsSync(), isCreate ? effected : !effected);
          if (isCreate && effected) {
            expect(
              sha256.convert(target.readAsBytesSync()).toString(),
              sha256.convert(prepared.content).toString(),
            );
          }
          if (!isCreate && !effected) {
            expect(target.readAsBytesSync(), prepared.content);
          }
          if (!expectCrash) {
            // A successful control still owns its selected target. Release it
            // and wait for real worker exit before installing unrelated bytes.
            await prepared.session.release();
            final exit = Stopwatch()..start();
            while (!prepared.session.canAcknowledge &&
                exit.elapsed < const Duration(seconds: 25)) {
              await prepared.session.refresh();
              await Future<void>.delayed(const Duration(milliseconds: 10));
            }
            expect(prepared.session.canAcknowledge, isTrue);
            await prepared.session.acknowledge();
          }
          preserved = Uint8List.fromList([7, 1, 9, 3, 5, 2]);
          if (!deferSentinel) {
            target.writeAsBytesSync(preserved, flush: true);
          }

          if (crashedCurrent) {
            try {
              await backend!.close();
            } catch (_) {
              // Its exit code has already been proven to be 86.
            }
          } else {
            await backend!.close();
          }
          backend = null;
          crashedCurrent = false;
          backend = await RustWorkbench.open(
            executable: normalHost,
            package: builtin,
            directory: directory!,
            managed: true,
          );
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
        await _awaitUi(tester, () => session.canStart, stage: 'local recovery');

        if (!isCreate) {
          final field = _control('disposition');
          await tester.tap(field);
          await tester.pump(const Duration(milliseconds: 300));
          await tester.tap(
            find
                .text(L10n.forLocale(const Locale('en')).pluginsIoFileDelete)
                .last,
          );
          await tester.pump(const Duration(milliseconds: 300));
        }
        final packageField = _control('package');
        await tester.tap(packageField);
        await tester.pump(const Duration(milliseconds: 300));
        await tester.tap(find.text(plugin.name).last);
        await tester.pump(const Duration(milliseconds: 300));
        await _tap(tester, 'guest-scope');
        expect(
          tester.widget<TextField>(_control('subject')).controller!.text,
          subject,
        );
        await _awaitUi(
          tester,
          () => _enabled(tester, 'discover'),
          stage: 'scope',
        );
        await _tap(tester, 'discover');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          stage: 'page ready',
        );
        expect(
          session.page,
          isNull,
        ); // A status refresh never reads implicitly.
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => _control('plan-0').evaluate().isNotEmpty,
          stage: 'original plan discovered',
        );
        expect(session.page!.plans, hasLength(1));
        expect(session.page!.plans!.single, plan);
        expect(session.page!.phase, MutationPhase.none);
        expect(session.page!.effect, MutationEffect.unspecified);
        await tester.ensureVisible(_control('plan-0'));
        await tester.tap(_control('plan-0'));
        await tester.pump();
        expect(view.selectedPlan, plan);
        expect(view.selectedRequest!.subject, subject);
        expect(view.selectedRequest!.disposition, disposition);
        expect(_enabled(tester, 'reconcile'), isFalse);

        // The selected original plan survives remount; neither mount nor a
        // status poll has authority to replay the guest's file operation.
        final originalDiscovery = session.discoveryRequest;
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpWidget(_screen(active, plugin, revision));
        await tester.pump();
        expect(session.discoveryRequest, same(originalDiscovery));
        expect(view.selectedPlan, plan);
        if (deferSentinel) {
          expect(target.existsSync(), isFalse);
        } else {
          expect(target.readAsBytesSync(), preserved);
        }

        await _awaitUi(
          tester,
          () => _enabled(tester, 'release'),
          stage: 'release',
        );
        await _tap(tester, 'release');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          stage: 'release receipt',
        );
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'ack'),
          stage: 'scan exit',
        );
        await _tap(tester, 'ack');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'reconcile'),
          stage: 'reconcile',
        );
        if (deferSentinel) {
          expect(target.existsSync(), isFalse);
        } else {
          expect(target.readAsBytesSync(), preserved);
        }

        await _tap(tester, 'reconcile');
        await _awaitUi(
          tester,
          () => _enabled(tester, 'read'),
          stage: 'history receipt',
        );
        await _tap(tester, 'read');
        await _awaitUi(
          tester,
          () => view.lastReconciliation != null,
          stage: 'historical result',
        );
        final result = view.lastReconciliation!;
        final observed = !expectCrash || point == 'after-observe';
        expect(result.operationId, operationId);
        expect(result.record, isNotEmpty);
        expect(result.outcome, observed ? isNotEmpty : isNull);
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
        if (deferSentinel) {
          expect(target.existsSync(), isFalse);
        } else {
          expect(target.readAsBytesSync(), preserved);
        }
        await _awaitUi(
          tester,
          () => _enabled(tester, 'ack'),
          stage: 'reconcile exit',
        );
        await _tap(tester, 'ack');
        await _awaitUi(
          tester,
          () => session.canStart,
          stage: 'reclaimed local',
        );
        if (deferSentinel) {
          expect(target.existsSync(), isFalse);
          target.writeAsBytesSync(preserved, flush: true);
          // Even a later remount/status refresh cannot resume Execute.
          await tester.pumpWidget(const SizedBox.shrink());
          await tester.pumpWidget(_screen(active, plugin, revision));
          await _awaitUi(
            tester,
            () => session.canStart,
            stage: 'second local view',
          );
        }
        expect(target.readAsBytesSync(), preserved);
        debugPrintSynchronously(
          'GUEST_CRASH_WIDGET_PASS=$kind:$point:${expectCrash ? 'crash' : 'normal'}',
        );
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        final current = backend;
        var exited = current == null;
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
            final observedExit = await tester.runAsync<int?>(() async {
              try {
                return await current.process.exitCode.timeout(
                  const Duration(seconds: 5),
                );
              } catch (_) {
                return null;
              }
            });
            exited = observedExit != null;
          } catch (_) {
            // Unknown process lifetime means the protected Store is retained.
          }
        }
        if (directory != null) {
          if (exited) {
            try {
              await tester.runAsync(() => removeTestDirectory(directory!));
            } catch (error, stack) {
              if (closeError == null) Error.throwWithStackTrace(error, stack);
              debugPrintSynchronously(
                'GUEST_CRASH_WIDGET_CLEANUP_FAILED=${directory!.path}: $error',
              );
            }
          } else {
            debugPrintSynchronously(
              'GUEST_CRASH_WIDGET_STORE_PRESERVED=${directory!.path}',
            );
          }
        }
        if (closeError != null) {
          Error.throwWithStackTrace(closeError, closeStack!);
        }
      }
    },
    skip:
        !Platform.isWindows ||
        faultHost == null ||
        normalHost == null ||
        builtin == null ||
        fixture == null ||
        kind == null ||
        point == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
