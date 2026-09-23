import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';
import 'package:morrow_studio/plugins/workbench_recovery.dart';

void main() {
  testWidgets(
    'timeout keeps owner; recovery renders and late close cannot touch the next session',
    (t) async {
      final session = SessionCoordinator(
        interactionDeadline: const Duration(milliseconds: 20),
      );
      addTearDown(session.dispose);
      final exit = Completer<int>(), close = Completer<void>();
      final process = Object();
      var opens = 0, closes = 0;
      await session.run(() async {
        opens++;
        session.attach(
          process: process,
          library: 'library-a',
          exited: exit.future,
          close: () {
            closes++;
            return close.future;
          },
        );
      });
      final pending = session.close();
      expect(identical(session.close(), pending), isTrue);
      var retries = 0;
      await t.pumpWidget(
        WorkbenchRecovery(
          session: session,
          locale: const Locale('en'),
          onRetry: () async {
            retries++;
          },
        ),
      );
      await t.pumpAndSettle();
      expect(session.phase, SessionPhase.closingUnconfirmed);
      expect(session.owner, same(process));
      expect(session.libraryLocation, 'library-a');
      await expectLater(
        session.run(() async {
          opens++;
        }),
        throwsStateError,
      );
      expect(opens, 1);
      expect(closes, 1);
      expect(
        t.widget<FilledButton>(find.byType(FilledButton).first).onPressed,
        isNull,
      );
      expect(
        find.textContaining('Shutdown is still unconfirmed'),
        findsOneWidget,
      );
      // The process fact, not the close future, allows a new checked open.
      exit.complete(7);
      await t.pumpAndSettle();
      expect(session.mayRecover, isTrue);
      expect(session.phase, SessionPhase.recoveryRequired);
      expect(
        t.widget<FilledButton>(find.byType(FilledButton).first).onPressed,
        isNotNull,
      );
      final next = Object(), nextExit = Completer<int>();
      await session.run(() async {
        session.attach(
          process: next,
          library: 'library-b',
          exited: nextExit.future,
          close: () async {},
        );
        session.active();
      });
      close.completeError(StateError('late old close failure'));
      await expectLater(pending, throwsStateError);
      expect(session.owner, same(next));
      expect(session.closeError, isNull);
      expect(session.phase, SessionPhase.active);
      expect(retries, 0);
      await t.pumpWidget(const SizedBox());
      nextExit.complete(0);
    },
  );
  test(
    'opening is single flight and a close success does not prove exit',
    () async {
      final session = SessionCoordinator(
        interactionDeadline: const Duration(milliseconds: 5),
      );
      final gate = Completer<void>(), exited = Completer<int>();
      var calls = 0;
      final first = session.run(() async {
        calls++;
        session.attach(
          process: Object(),
          library: 'a',
          exited: exited.future,
          close: () async {},
        );
        await gate.future;
      });
      expect(
        identical(
          session.run(() async {
            calls++;
          }),
          first,
        ),
        isTrue,
      );
      gate.complete();
      await first;
      await session.close();
      await Future<void>.delayed(const Duration(milliseconds: 15));
      expect(calls, 1);
      expect(session.mayRecover, isFalse);
      expect(session.phase, SessionPhase.closingUnconfirmed);
      exited.complete(0);
      await Future<void>.delayed(Duration.zero);
      session.dispose();
    },
  );
}
