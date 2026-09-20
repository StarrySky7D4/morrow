import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_session.dart';

import 'service_run_real_native_fixture.dart';

Future<void> usableOriginalStore(RealServiceFixture fixture) async {
  expect(await fixture.backend.readUiLocale(), 'zh');
  await fixture.backend.saveUiLocale('en');
  expect(await fixture.backend.readUiLocale(), 'en');
}

void main() {
  final skip = !RealServiceFixture.available;
  test(
    'occupied Windows port reports bind failure, returns original store, and retains task until acknowledgement',
    () async {
      final f = await RealServiceFixture.open(occupyPort: true);
      try {
        await f.session.start(f.request());
        expect(f.session.startUnknown, isFalse);
        final key = f.session.task!.key;
        final exited = await f.observe(ServiceRunPhase.exited);
        expect(exited.bind, ServiceNetworkOutcome.transport);
        expect(exited.task.storage, IoStoragePhase.reclaimed);
        expect(exited.task.exit, isNotNull);
        expect(exited.task.key, key);
        expect(f.session.canAcknowledge, isTrue);
        expect(f.session.canStart, isFalse);
        await usableOriginalStore(f);
        await f.session.acknowledge();
        expect(f.session.notice, isNull);
        expect(f.session.task!.key, isNull);
        expect(f.session.canStart, isTrue);
      } finally {
        await f.close();
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(minutes: 1)),
  );

  test(
    'short finite lifetime stops the actual listener and reclaims owner without a stop request',
    () async {
      final f = await RealServiceFixture.open();
      try {
        final watch = Stopwatch()..start();
        await f.session.start(f.request(lifetime: 1200, timeout: 250));
        final live = await f.observe(ServiceRunPhase.running);
        final key = live.task.key;
        expect(live.bind, ServiceNetworkOutcome.succeeded);
        await Future<void>.delayed(const Duration(milliseconds: 300));
        await f.session.refresh();
        expect(f.session.trusted, isTrue);
        expect(f.session.service!.phase, ServiceRunPhase.running);
        expect(f.session.service!.submission, live.submission);
        final exited = await f.observe(ServiceRunPhase.exited);
        expect(
          watch.elapsed,
          greaterThanOrEqualTo(const Duration(milliseconds: 1100)),
        );
        expect(watch.elapsed, lessThan(const Duration(seconds: 15)));
        expect(exited.task.storage, IoStoragePhase.reclaimed);
        expect(exited.task.exit, isNotNull);
        expect(exited.task.key, key);
        await f.session.refresh();
        expect(f.session.task!.key, key);
        expect(f.session.canAcknowledge, isTrue);
        await expectLater(
          f.post(second: true),
          throwsA(isA<SocketException>()),
        );
        await usableOriginalStore(f);
        await f.session.acknowledge();
        expect(f.session.task!.storage, IoStoragePhase.local);
        expect(f.session.task!.key, isNull);
      } finally {
        await f.close();
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(minutes: 1)),
  );

  for (final authentication in [false, true]) {
    test(
      'ordinary business API revokes ${authentication ? 'authentication' : 'publication'} during real service execution',
      () async {
        final f = await RealServiceFixture.open();
        try {
          await f.session.start(f.request());
          final live = await f.observe(ServiceRunPhase.running);
          final first = await f.post();
          expect(first, startsWith('HTTP/1.1 202 '));
          expect(first, endsWith('executed-before'));
          final authority = authentication
              ? f.issued!.authority
              : f.publication;
          try {
            final disabled = await f.backend.disableServiceAuthority(
              authority.reference,
              authority.revision,
            );
            expect(disabled.disabled, isTrue);
          } on ServiceCommandFailure catch (error) {
            // Invalidating the live authority can stop reply delivery after the
            // write. Reconcile the original record after exit; never retry it.
            expect(error.outcomeUnknown, isTrue);
            expect(error.task, live.task.key);
            expect(error.submission, hasLength(32));
            expect(error.command, hasLength(32));
          }
          final exited = await f.observe(ServiceRunPhase.exited);
          expect(exited.task.key, live.task.key);
          expect(exited.task.storage, IoStoragePhase.reclaimed);
          expect(exited.task.exit, isNotNull);
          await expectLater(
            f.post(second: true),
            throwsA(isA<SocketException>()),
          );
          final page = await f.backend.serviceAuthorityPage();
          final stored = page.records.singleWhere(
            (a) => a.kind == authority.kind,
          );
          expect(stored.reference, authority.reference);
          expect(stored.disabled, isTrue);
          expect(stored.revision, authority.revision + BigInt.one);
          await usableOriginalStore(f);
          expect(f.session.canAcknowledge, isTrue);
          await f.session.acknowledge();
          expect(f.session.task!.storage, IoStoragePhase.local);
        } finally {
          await f.close();
        }
      },
      skip: skip,
      timeout: const Timeout(Duration(minutes: 1)),
    );
  }

  for (final invalid in ['registry', 'config', 'run-budget', 'worker-limits']) {
    test(
      'explicit $invalid admission rejection is observed before a new attempt is allowed',
      () async {
        final f = await RealServiceFixture.open();
        try {
          final request = f.request(
            registryRevision: invalid == 'registry'
                ? f.registry + BigInt.one
                : null,
            configRevision: invalid == 'config'
                ? f.config.revision + BigInt.one
                : null,
            jobs: invalid == 'run-budget' ? 65 : 64,
            calls: invalid == 'worker-limits' ? 64 : 4,
          );
          await f.session.start(request);
          expect(f.session.notice, ServiceRunNotice.startRejected);
          expect(f.session.startFailureDetail, isNotEmpty);
          expect(f.session.startUnknown, isFalse);
          expect(f.session.trusted, isTrue);
          if (invalid == 'worker-limits') {
            expect(f.session.task!.key, isNotNull);
            expect(f.session.service!.submission, request.submission);
            expect(f.session.canStart, isFalse);
          }
          // A rejection is not itself evidence that cleanup left no task.
          if (f.session.task!.key != null) {
            final exited = await f.observe(ServiceRunPhase.exited);
            expect(exited.submission, request.submission);
            expect(exited.task.exit, isNotNull);
            expect(f.session.canAcknowledge, isTrue);
            await f.session.acknowledge();
          }
          expect(f.session.task!.storage, IoStoragePhase.local);
          expect(f.session.task!.key, isNull);
          expect(f.session.canStart, isTrue);
          expect(f.session.history.last.request.submission, request.submission);
          expect(f.session.history.last.outcomeUnknown, isFalse);
          await usableOriginalStore(f);
        } finally {
          await f.close();
        }
      },
      skip: skip,
      timeout: const Timeout(Duration(minutes: 1)),
    );
  }
}
