import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_owner_management.dart';
import 'package:morrow_studio/plugins/workbench_recovery.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';

Map<String, dynamic> record() => {
  'version': 2,
  'phase': 'Recovered',
  'profile': 'a' * 64,
  'generation': 1,
  'incarnation': 'b' * 64,
  'supervisor_pid': 100,
  'child_pid': 101,
  'supervisor_creation_filetime': 10,
  'child_creation_filetime': 11,
  'host_sha256': 'e' * 64,
  'package_sha256': 'f' * 64,
  'supervisor_sha256': '1' * 64,
  'protocol_ack': 'not_applicable',
  'child_exit_code': 2,
  'business_outcome': 'Unknown',
  'normal_shutdown': false,
  'business_gate_revoked': true,
  for (final key in ownerResourceProof) key: true,
};
WorkbenchOwnerPreview preview({bool eligible = true}) => WorkbenchOwnerPreview({
  'eligible': eligible,
  'reason': eligible
      ? 'eligible_resource_only'
      : 'original_resource_proof_missing',
  'preview_token': 'c' * 64,
  'original_digest': 'd' * 64,
  'record': record(),
});
WorkbenchOwnerRecovery recovered() =>
    WorkbenchOwnerRecovery({'recovered': true, 'record': record()});
WorkbenchOwnerPreview confirmedPreview() => WorkbenchOwnerPreview({
  'version': 1,
  'profile': 'a' * 64,
  'eligible': false,
  'reason': 'already_resource_recovered',
  'preview_token': 'c' * 64,
  'original_digest': '2' * 64,
  'resource_recovery_confirmed': true,
  'recovery_original_digest': 'd' * 64,
  'history_head': '3' * 64,
  'business_outcome': 'Unknown',
  'record': {
    ...record(),
    'recovery': {
      'version': 1,
      'original_digest': 'd' * 64,
      'receipt_digest': '4' * 64,
      'history_head': '3' * 64,
    },
  },
});

Future<SessionCoordinator> failedSession(
  WidgetTester t, {
  bool transportEnded = true,
}) async {
  final session = SessionCoordinator();
  addTearDown(session.dispose);
  final actual = Completer<int>(), verified = Completer<int>();
  await session.run(() async {
    session.attach(
      process: Object(),
      library: 'library',
      exited: verified.future,
      transportExited: actual.future,
      close: () async {},
    );
  });
  if (transportEnded) actual.complete(2);
  verified.completeError(StateError('Unknown'));
  await t.pump();
  return session;
}

void main() {
  test(
    'durable read-only confirmation requires native marker and complete bindings',
    () {
      final p = confirmedPreview();
      expect(p.confirmedRecovery!.hasOriginalResourceProof, isTrue);
      final alterations = <void Function(Map<String, dynamic>)>[
        (v) => v.remove('resource_recovery_confirmed'),
        (v) => v['recovery_original_digest'] = '0' * 64,
        (v) => v['history_head'] = '0' * 64,
        (v) => v['profile'] = '0' * 64,
        (v) => v['eligible'] = true,
        (v) => (v['record'] as Map)['version'] = 1,
        (v) => (v['record'] as Map)['normal_shutdown'] = true,
        (v) => (v['record'] as Map)['business_outcome'] = 'Observed',
        (v) => (v['record'] as Map)['supervisor_creation_filetime'] = 0,
        (v) => (v['record'] as Map)['business_gate_revoked'] = false,
        for (final key in ownerResourceProof)
          (v) => (v['record'] as Map)[key] = false,
      ];
      for (final alter in alterations) {
        final v = confirmedPreview().value;
        alter(v);
        expect(
          () => WorkbenchOwnerPreview(v).confirmedRecovery,
          throwsA(isA<WorkbenchOwnerManagementUnconfirmed>()),
        );
      }
      final normal = confirmedPreview().value;
      normal['reason'] = 'already_normal_released';
      (normal['record'] as Map)['phase'] = 'Released';
      expect(WorkbenchOwnerPreview(normal).confirmedRecovery, isNull);
    },
  );

  testWidgets(
    'uncertain commit re-preview confirms resources but only manual Retry opens a new owner',
    (t) async {
      final session = await failedSession(t);
      final epoch = session.generation;
      var checks = 0, repairs = 0, retries = 0;
      final newOwner = Object();
      await t.pumpWidget(
        WorkbenchRecovery(
          locale: const Locale('en'),
          session: session,
          onPreviewOwner: () async {
            checks++;
            if (checks == 1) return preview();
            final p = confirmedPreview();
            session.acceptResourceRecovery(
              p.confirmedRecovery!,
              expectedGeneration: epoch,
              expectedProfile: 'a' * 64,
            );
            return p;
          },
          onRecoverOwner: (_) async {
            repairs++;
            throw const WorkbenchOwnerManagementUnconfirmed(
              'recovery_result_unconfirmed',
            );
          },
          onRetry: () => session.run(() async {
            retries++;
            session.attach(
              process: newOwner,
              library: 'library',
              exited: Completer<int>().future,
              transportExited: Completer<int>().future,
              close: () async {},
            );
          }),
        ),
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('recover-owner')));
      await t.pumpAndSettle();
      await t.tap(find.byType(Checkbox));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('confirm-owner-recovery')));
      await t.pumpAndSettle();
      expect(repairs, 1);
      expect(retries, 0);
      expect(session.mayRecover, isFalse);
      expect(
        find.textContaining('commit or output is unconfirmed'),
        findsOneWidget,
      );
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      expect(checks, 2);
      expect(repairs, 1);
      expect(retries, 0);
      expect(session.owner, isNull);
      expect(session.exitCode, isNull);
      expect(session.transportExitCode, 2);
      expect(session.closeError, isNotNull);
      expect(
        find.textContaining('Select Retry to open a new session'),
        findsNWidgets(2),
      );
      await t.ensureVisible(find.byKey(const ValueKey('retry-workbench')));
      await t.tap(find.byKey(const ValueKey('retry-workbench')));
      await t.pumpAndSettle();
      expect(retries, 1);
      expect(session.generation, epoch + 1);
      expect(session.owner, same(newOwner));
      expect(session.mayRecover, isFalse);
      expect(
        () => session.attach(
          process: Object(),
          library: 'library',
          exited: Completer<int>().future,
          close: () async {},
        ),
        throwsStateError,
      );
      expect(
        () => session.acceptResourceRecovery(
          confirmedPreview().confirmedRecovery!,
          expectedGeneration: epoch,
          expectedProfile: 'a' * 64,
        ),
        throwsA(isA<WorkbenchOwnerManagementUnconfirmed>()),
      );
      expect(session.owner, same(newOwner));
    },
  );

  testWidgets(
    'live UI transport permits preview but blocks repair and Retry with diagnostic',
    (t) async {
      final session = await failedSession(t, transportEnded: false);
      var repairs = 0, retries = 0;
      await t.pumpWidget(
        WorkbenchRecovery(
          locale: const Locale('en'),
          session: session,
          onPreviewOwner: () async => preview(),
          onRecoverOwner: (_) async {
            repairs++;
            return recovered();
          },
          onRetry: () async {
            retries++;
          },
        ),
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      expect(
        find.textContaining('original UI transport has not exited'),
        findsOneWidget,
      );
      expect(
        t
            .widget<OutlinedButton>(find.byKey(const ValueKey('recover-owner')))
            .onPressed,
        isNull,
      );
      expect(
        t
            .widget<FilledButton>(find.byKey(const ValueKey('retry-workbench')))
            .onPressed,
        isNull,
      );
      expect(repairs, 0);
      expect(retries, 0);
    },
  );

  testWidgets(
    'replacing the coordinator discards the old preview even at the same generation',
    (t) async {
      final first = SessionCoordinator(), second = SessionCoordinator();
      addTearDown(first.dispose);
      addTearDown(second.dispose);
      WorkbenchRecovery view(SessionCoordinator session) => WorkbenchRecovery(
        locale: const Locale('en'),
        session: session,
        onRetry: () async {},
        onPreviewOwner: () async => preview(),
        onRecoverOwner: (_) async {
          fail('old preview must be discarded');
        },
      );
      await t.pumpWidget(view(first));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('recover-owner')), findsOneWidget);
      await t.pumpWidget(view(second));
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('recover-owner')), findsNothing);
    },
  );

  testWidgets(
    'session change while confirmation dialog is open refuses the old preview',
    (t) async {
      final session = SessionCoordinator();
      addTearDown(session.dispose);
      var repairs = 0;
      await t.pumpWidget(
        WorkbenchRecovery(
          locale: const Locale('en'),
          session: session,
          onPreviewOwner: () async => preview(),
          onRecoverOwner: (_) async {
            repairs++;
            return recovered();
          },
          onRetry: () async {},
        ),
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('recover-owner')));
      await t.pumpAndSettle();
      await session.run(() async {});
      await t.pump();
      await t.tap(find.byType(Checkbox));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('confirm-owner-recovery')));
      await t.pumpAndSettle();
      expect(repairs, 0);
      expect(find.textContaining('preview is stale'), findsOneWidget);
      expect(find.byKey(const ValueKey('recover-owner')), findsNothing);
    },
  );

  for (final reason in [
    'artifact_binding_mismatch',
    'stale_owner_preview',
    'ui_transport_still_live',
  ]) {
    testWidgets('$reason produces a clear refusal without a recovery action', (
      t,
    ) async {
      await t.pumpWidget(
        WorkbenchRecovery(
          locale: const Locale('en'),
          onRetry: () async {},
          onPreviewOwner: () async {
            throw WorkbenchOwnerManagementUnconfirmed(reason);
          },
        ),
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('recover-owner')), findsNothing);
      expect(
        find.textContaining(switch (reason) {
          'artifact_binding_mismatch' => 'artifacts differ',
          'stale_owner_preview' => 'preview is stale',
          _ => 'UI transport has not exited',
        }),
        findsOneWidget,
      );
    });
  }

  test(
    'another original incarnation cannot clear the retained owner',
    () async {
      final session = SessionCoordinator();
      addTearDown(session.dispose);
      final actual = Completer<int>(), verified = Completer<int>();
      final originalOwner = Object();
      await session.run(() async {
        session.attach(
          process: originalOwner,
          library: 'library',
          exited: verified.future,
          transportExited: actual.future,
          close: () async {},
        );
      });
      actual.complete(2);
      verified.completeError(StateError('Unknown'));
      await Future<void>.delayed(Duration.zero);
      session.observeSupervision(
        WorkbenchSupervisionState({...record(), 'phase': 'ClosingUnconfirmed'}),
      );
      final foreign = recovered();
      foreign.record['incarnation'] = '0' * 64;
      expect(
        () => session.acceptResourceRecovery(
          foreign,
          expectedGeneration: session.generation,
          expectedProfile: 'a' * 64,
        ),
        throwsStateError,
      );
      expect(session.owner, same(originalOwner));
      expect(session.exitCode, isNull);
      expect(session.transportExitCode, 2);
      expect(session.mayRecover, isFalse);
    },
  );

  test(
    'explicit resource recovery retains failed exit and refuses stale UI generation',
    () async {
      final session = SessionCoordinator();
      addTearDown(session.dispose);
      final actual = Completer<int>(), verified = Completer<int>();
      await session.run(() async {
        session.attach(
          process: Object(),
          library: 'library',
          exited: verified.future,
          transportExited: actual.future,
          close: () async {},
        );
      });
      final epoch = session.generation;
      expect(
        () => session.acceptResourceRecovery(
          recovered(),
          expectedGeneration: epoch,
          expectedProfile: 'a' * 64,
        ),
        throwsA(isA<WorkbenchOwnerManagementUnconfirmed>()),
      );
      actual.complete(2);
      verified.completeError(StateError('Unknown'));
      await Future<void>.delayed(Duration.zero);
      expect(
        () => session.acceptResourceRecovery(
          recovered(),
          expectedGeneration: epoch + 1,
          expectedProfile: 'a' * 64,
        ),
        throwsA(isA<WorkbenchOwnerManagementUnconfirmed>()),
      );
      session.acceptResourceRecovery(
        recovered(),
        expectedGeneration: epoch,
        expectedProfile: 'a' * 64,
      );
      expect(session.mayRecover, isTrue);
      expect(session.exitCode, isNull);
      expect(session.transportExitCode, 2);
      expect(session.closeError, isNotNull);
      expect(session.phase, SessionPhase.recoveryRequired);
    },
  );
  testWidgets(
    'read-only preview stays available while retry blocked; recovery requires explicit Unknown acknowledgment',
    (t) async {
      final session = SessionCoordinator();
      addTearDown(session.dispose);
      final actual = Completer<int>(), verified = Completer<int>();
      await session.run(() async {
        session.attach(
          process: Object(),
          library: 'library',
          exited: verified.future,
          transportExited: actual.future,
          close: () async {},
        );
      });
      actual.complete(2);
      verified.completeError(StateError('Unknown'));
      await t.pump();
      var repairs = 0, retries = 0;
      await t.pumpWidget(
        WorkbenchRecovery(
          locale: const Locale('en'),
          session: session,
          onRetry: () async {
            retries++;
          },
          onPreviewOwner: () async => preview(),
          onRecoverOwner: (p) async {
            repairs++;
            session.acceptResourceRecovery(
              recovered(),
              expectedGeneration: session.generation,
              expectedProfile: 'a' * 64,
            );
            return recovered();
          },
        ),
      );
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('recover-owner')));
      await t.pumpAndSettle();
      expect(
        t
            .widget<FilledButton>(
              find.byKey(const ValueKey('confirm-owner-recovery')),
            )
            .onPressed,
        isNull,
      );
      expect(repairs, 0);
      await t.tap(find.byType(Checkbox));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('confirm-owner-recovery')));
      await t.pumpAndSettle();
      expect(repairs, 1);
      expect(retries, 0);
      expect(
        find.textContaining('original outcomes remain Unknown'),
        findsOneWidget,
      );
      await t.ensureVisible(find.byType(FilledButton));
      await t.tap(find.byType(FilledButton));
      await t.pumpAndSettle();
      expect(retries, 1);
    },
  );
  testWidgets('missing proof shows diagnostic and has no recover action', (
    t,
  ) async {
    await t.pumpWidget(
      WorkbenchRecovery(
        locale: const Locale('en'),
        onRetry: () async {},
        onPreviewOwner: () async => preview(eligible: false),
        onRecoverOwner: (_) async {
          fail('must not recover');
        },
      ),
    );
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('preview-owner-recovery')));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('recover-owner')), findsNothing);
    expect(
      find.textContaining('Process absence or timeout cannot replace'),
      findsOneWidget,
    );
  });
}
