import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';
import 'package:morrow_studio/plugins/workbench_recovery.dart';

Map<String, dynamic> status() => {
  'version': 1,
  'phase': 'Released',
  'owner_phase': 'Released',
  'profile': 'a' * 64,
  'incarnation': 'b' * 64,
  'host_sha256': 'c' * 64,
  'package_sha256': 'd' * 64,
  'generation': 1,
  'child_pid': 12,
  'supervisor_pid': 13,
  'protocol_ack': 'not_applicable',
  'business_outcome': 'Unknown',
  'normal_shutdown': true,
  'durable_proof': true,
  'child_exit_code': 0,
  for (final k in [
    'resource_reclaimed',
    'tree_empty',
    'child_exit',
    'stdout_eof',
    'stderr_eof',
    'control_reaped',
    'gate_closed',
  ])
    k: true,
};
void main() {
  test(
    'ACK, process exit and resource proof cannot substitute for durable owner release',
    () {
      expect(
        WorkbenchSupervisionState.decode(
          utf8.encode(jsonEncode(status())),
        ).released,
        isTrue,
      );
      for (final key in [
        'resource_reclaimed',
        'tree_empty',
        'child_exit',
        'stdout_eof',
        'stderr_eof',
        'control_reaped',
        'gate_closed',
        'normal_shutdown',
        'durable_proof',
      ]) {
        final invalid = status()..[key] = false;
        expect(
          () => WorkbenchSupervisionState.decode(
            utf8.encode(jsonEncode(invalid)),
          ),
          throwsFormatException,
          reason: key,
        );
      }
      final invalid = status()..['protocol_ack'] = 'received';
      expect(
        () =>
            WorkbenchSupervisionState.decode(utf8.encode(jsonEncode(invalid))),
        throwsFormatException,
      );
    },
  );
  testWidgets(
    'wrapper crash permits window exit but keeps recovery blocked and Unknown visible',
    (t) async {
      final session = SessionCoordinator();
      addTearDown(session.dispose);
      final actual = Completer<int>(), proven = Completer<int>();
      await session.run(() async {
        session.attach(
          process: Object(),
          library: 'original',
          exited: proven.future,
          transportExited: actual.future,
          close: () async {},
        );
        session.active();
      });
      actual.complete(2);
      proven.completeError(
        const WorkbenchSupervisionUnconfirmed('No final proof'),
      );
      await t.pump();
      expect(session.mayRecover, isFalse);
      expect(session.mayCloseWindow, isTrue);
      expect(session.phase, SessionPhase.closingUnconfirmed);
      await t.pumpWidget(
        WorkbenchRecovery(
          locale: const Locale('zh'),
          failure: session.closeError,
          session: session,
          onRetry: () async {
            fail('must not retry');
          },
        ),
      );
      await t.pumpAndSettle();
      expect(find.textContaining('操作结果未知'), findsOneWidget);
      expect(find.textContaining('所有权尚未确认释放'), findsOneWidget);
    },
  );
}
