import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/application_shutdown.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';
import 'package:morrow_studio/plugins/workbench_shutdown.dart';

void main() {
  testWidgets('background close keeps the owner until a real exit', (t) async {
    final session = SessionCoordinator(
      interactionDeadline: const Duration(milliseconds: 5),
    );
    addTearDown(session.dispose);
    final exited = Completer<int>();
    final closing = Completer<void>();
    final process = Object();
    var closeCalls = 0;
    await session.run(() async {
      session.attach(
        process: process,
        library: 'library-a',
        exited: exited.future,
        close: () {
          closeCalls++;
          return closing.future;
        },
      );
      session.active();
    });
    var views = 0, hides = 0, shows = 0, destroys = 0;
    final shutdown = ApplicationShutdown(
      session: session,
      showClosing: () => views++,
      hideWindow: () async {
        hides++;
      },
      showWindow: () async {
        shows++;
      },
      destroyWindow: () async {
        destroys++;
      },
      backgroundReminder: const Duration(milliseconds: 20),
    );
    session.addListener(shutdown.observe);
    addTearDown(() => session.removeListener(shutdown.observe));

    shutdown.request();
    await t.pump();
    expect(views, 1);
    expect(closeCalls, 1);
    expect(destroys, 0);
    await shutdown.continueInBackground();
    expect(hides, 1);
    expect(shutdown.hidden, isTrue);
    expect(session.owner, same(process));
    expect(session.mayRecover, isFalse);
    await t.pump(const Duration(milliseconds: 25));
    expect(shows, 1);
    expect(shutdown.hidden, isFalse);
    expect(destroys, 0);

    shutdown.request(); // The close button on the shutdown view hides it.
    await t.pump();
    expect(hides, 2);
    expect(closeCalls, 1);
    exited.complete(0);
    await t.pump();
    expect(destroys, 1);
    expect(session.mayRecover, isTrue);
    closing.complete();
    await t.pump();
    expect(destroys, 1);
  });

  testWidgets('close failure restores a hidden shutdown view', (t) async {
    final session = SessionCoordinator();
    addTearDown(session.dispose);
    final exited = Completer<int>(), closing = Completer<void>();
    final process = Object();
    await session.run(() async {
      session.attach(
        process: process,
        library: 'library-a',
        exited: exited.future,
        close: () => closing.future,
      );
    });
    var shows = 0, destroys = 0;
    final shutdown = ApplicationShutdown(
      session: session,
      showClosing: () {},
      hideWindow: () async {},
      showWindow: () async {
        shows++;
      },
      destroyWindow: () async {
        destroys++;
      },
    );
    session.addListener(shutdown.observe);
    addTearDown(() => session.removeListener(shutdown.observe));
    shutdown.request();
    await shutdown.continueInBackground();
    closing.completeError(StateError('close failed'));
    await t.pump();
    expect(shows, 1);
    expect(shutdown.hidden, isFalse);
    expect(session.owner, same(process));
    expect(session.mayRecover, isFalse);
    expect(destroys, 0);
    // The reported failure can be acknowledged by hiding again.
    await shutdown.continueInBackground();
    expect(shutdown.hidden, isTrue);
    exited.complete(7);
    await t.pump();
    expect(destroys, 1);
  });

  testWidgets('shutdown view offers background close and reports delay', (
    t,
  ) async {
    final session = SessionCoordinator(
      interactionDeadline: const Duration(milliseconds: 5),
    );
    addTearDown(session.dispose);
    final exited = Completer<int>(), closing = Completer<void>();
    await session.run(() async {
      session.attach(
        process: Object(),
        library: 'library-a',
        exited: exited.future,
        close: () => closing.future,
      );
    });
    var background = 0;
    await t.pumpWidget(
      WorkbenchShutdown(
        session: session,
        locale: const Locale('en'),
        onBackground: () async {
          background++;
        },
      ),
    );
    await t.pumpAndSettle();
    expect(find.text('Closing workspace'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('shutdown-background')));
    expect(background, 1);
    session.close();
    await t.pump(const Duration(milliseconds: 10));
    expect(find.textContaining('longer than expected'), findsOneWidget);
    closing.complete();
    exited.complete(0);
    await t.pump();
    await t.pumpWidget(const SizedBox());
  });
  testWidgets('deferred hide finishes before error reveals the window', (
    t,
  ) async {
    final session = SessionCoordinator();
    addTearDown(session.dispose);
    final exited = Completer<int>(), closing = Completer<void>();
    await session.run(() async {
      session.attach(
        process: Object(),
        library: 'library-a',
        exited: exited.future,
        close: () => closing.future,
      );
    });
    final hiding = Completer<void>();
    var shows = 0;
    final shutdown = ApplicationShutdown(
      session: session,
      showClosing: () {},
      hideWindow: () => hiding.future,
      showWindow: () async {
        shows++;
      },
      destroyWindow: () async {},
    );
    session.addListener(shutdown.observe);
    addTearDown(() => session.removeListener(shutdown.observe));
    shutdown.request();
    final hide = shutdown.continueInBackground();
    closing.completeError(StateError('shutdown failed'));
    await t.pump();
    expect(shows, 0);
    hiding.complete();
    await hide;
    await t.pump();
    expect(shows, 1);
    expect(shutdown.hidden, isFalse);
    exited.complete(0);
    await t.pump();
  });

  testWidgets('deferred hide finishes before confirmed exit destroys window', (
    t,
  ) async {
    final session = SessionCoordinator();
    addTearDown(session.dispose);
    final exited = Completer<int>(), closing = Completer<void>();
    await session.run(() async {
      session.attach(
        process: Object(),
        library: 'library-a',
        exited: exited.future,
        close: () => closing.future,
      );
    });
    final hiding = Completer<void>();
    var destroys = 0, shows = 0;
    final shutdown = ApplicationShutdown(
      session: session,
      showClosing: () {},
      hideWindow: () => hiding.future,
      showWindow: () async {
        shows++;
      },
      destroyWindow: () async {
        destroys++;
      },
    );
    session.addListener(shutdown.observe);
    addTearDown(() => session.removeListener(shutdown.observe));
    shutdown.request();
    final hide = shutdown.continueInBackground();
    exited.complete(0);
    await t.pump();
    expect(destroys, 0);
    hiding.complete();
    await hide;
    await t.pump();
    expect(destroys, 1);
    expect(shows, 0);
    closing.complete();
    await t.pump();
  });
  testWidgets('destroy failure stays visible and close can retry', (t) async {
    final session = SessionCoordinator();
    addTearDown(session.dispose);
    final exited = Completer<int>(), closing = Completer<void>();
    await session.run(() async {
      session.attach(
        process: Object(),
        library: 'library-a',
        exited: exited.future,
        close: () => closing.future,
      );
    });
    var views = 0, destroys = 0;
    final shutdown = ApplicationShutdown(
      session: session,
      showClosing: () {
        views++;
      },
      hideWindow: () async {},
      showWindow: () async {},
      destroyWindow: () async {
        destroys++;
        if (destroys == 1) throw StateError('destroy failed');
      },
    );
    session.addListener(shutdown.observe);
    addTearDown(() => session.removeListener(shutdown.observe));
    shutdown.request();
    exited.complete(0);
    await t.pump();
    expect(destroys, 1);
    expect(shutdown.destroying, isFalse);
    expect(shutdown.windowError, isA<StateError>());
    expect(views, 2);
    shutdown.request();
    await t.pump();
    expect(destroys, 2);
    expect(shutdown.destroying, isTrue);
    closing.complete();
    await t.pump();
  });
}
