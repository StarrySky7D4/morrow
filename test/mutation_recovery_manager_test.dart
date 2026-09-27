import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/mutation_recovery_manager.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/mutation_workflow.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

Uint8List _id(int byte) => Uint8List.fromList(List.filled(32, byte));

MutationResult _result(
  MutationResultKind kind, {
  List<Uint8List>? plans,
  Uint8List? checkpoint,
  int scanned = 0,
  bool done = false,
  MutationPhase phase = MutationPhase.none,
}) => MutationResult(
  kind: kind,
  plans: plans,
  checkpoint: checkpoint,
  stagedBytes: BigInt.zero,
  durableContent: false,
  phase: phase,
  operationId: null,
  effect: MutationEffect.unspecified,
  osCode: null,
  scanned: scanned,
  done: done,
);

MutationResult _page(List<int> plans, {bool done = false}) => _result(
  MutationResultKind.plans,
  plans: [
    for (final value in plans) Uint8List.fromList([value]),
  ],
  checkpoint: done ? null : _id(21),
  scanned: plans.isEmpty ? 1 : plans.length,
  done: done,
);

class _Backend implements MutationTaskBackend, WorkbenchIoTaskControl {
  Uint8List? key, submission;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  MutationCommandKind kind = MutationCommandKind.discover;
  MutationDelivery delivery = MutationDelivery.pending;
  BigInt command = BigInt.one;
  bool terminal = false, loseRead = false;
  MutationResult? pending;
  final queuedPages = <MutationResult>[];
  MutationResult reconcileResult = _result(
    MutationResultKind.reconciled,
    phase: MutationPhase.outcomeUnknown,
  );
  final discoveryRequests = <MutationDiscoverRequest>[];
  final reconciliationRequests = <MutationReconcileRequest>[];
  int statuses = 0, reads = 0, nexts = 0, releases = 0;
  int stops = 0, acks = 0, repairs = 0;

  IoTaskSnapshot _snapshot() => IoTaskSnapshot(
    key: key,
    submission: submission,
    storage: storage,
    delivery: switch (delivery) {
      MutationDelivery.pending => IoDeliveryPhase.pending,
      MutationDelivery.ready => IoDeliveryPhase.ready,
      MutationDelivery.consumed => IoDeliveryPhase.consumed,
    },
    exit: exit,
  );

  MutationTaskReply _reply({bool status = false}) => MutationTaskReply(
    io: _snapshot(),
    state: MutationState(
      commandId: command,
      kind: kind,
      delivery: delivery,
      selected: false,
      reconcileRequired: false,
      terminal: terminal,
    ),
    commandId: status ? BigInt.zero : command,
  );

  Future<MutationTaskReply> _start(
    Uint8List token,
    MutationCommandKind next,
  ) async {
    key = _id(3);
    submission = token;
    storage = IoStoragePhase.running;
    exit = null;
    command = BigInt.one;
    kind = next;
    delivery = MutationDelivery.ready;
    pending = next == MutationCommandKind.discover
        ? queuedPages.removeAt(0)
        : reconcileResult;
    terminal = pending!.done;
    return _reply();
  }

  @override
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request) {
    discoveryRequests.add(request);
    return _start(request.submission, MutationCommandKind.discover);
  }

  @override
  Future<MutationTaskReply> startReconciliation(
    MutationReconcileRequest request,
  ) {
    reconciliationRequests.add(request);
    return _start(request.submission, MutationCommandKind.reconcile);
  }

  @override
  Future<MutationTaskReply> submitNextPlans(
    Uint8List taskKey,
    Uint8List token,
    int scanLimit,
  ) async {
    nexts++;
    command += BigInt.one;
    kind = MutationCommandKind.discover;
    delivery = MutationDelivery.ready;
    pending = queuedPages.removeAt(0);
    terminal = pending!.done;
    return _reply();
  }

  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List taskKey,
    Uint8List token,
  ) async {
    releases++;
    command += BigInt.one;
    kind = MutationCommandKind.release;
    delivery = MutationDelivery.ready;
    terminal = true;
    pending = _result(MutationResultKind.released);
    return _reply();
  }

  @override
  Future<MutationTaskReply> status(Uint8List taskKey) async {
    statuses++;
    return _reply(status: true);
  }

  @override
  Future<MutationTaskRead> read(Uint8List taskKey, BigInt commandId) async {
    reads++;
    delivery = MutationDelivery.consumed;
    final value = pending;
    pending = null;
    if (loseRead) throw StateError('lost consumed read');
    return MutationTaskRead(reply: _reply(), result: value);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return _snapshot();
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List taskKey) async {
    stops++;
    storage = IoStoragePhase.stopping;
    return _snapshot();
  }

  void joined({bool repair = false}) {
    storage = repair
        ? IoStoragePhase.recoveryRequired
        : IoStoragePhase.reclaimed;
    exit = const IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: IoJobError.none,
    );
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List taskKey) async {
    repairs++;
    storage = IoStoragePhase.reclaimed;
    return _snapshot();
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    acks++;
    key = null;
    submission = null;
    storage = IoStoragePhase.local;
    exit = null;
    return _snapshot();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}

PluginLibraryEntry _plugin({bool authorized = true}) => PluginLibraryEntry(
  id: 'test.recovery-ui',
  name: 'Recovery test package',
  version: '1',
  digest: _id(2),
  enabled: true,
  builtin: false,
  available: true,
  declared: const [],
  approved: const [],
  dependencies: const [],
  handlers: const [],
  issue: '',
  declaredIo: const ['file-create'],
  approvedIo: authorized ? const ['file-create'] : const [],
);

Widget _screen(
  _Backend backend, {
  String locale = 'en',
  bool visible = true,
  bool authorized = true,
  BigInt? revision,
  MutationRecoverySeed? seed,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: TickerMode(
      enabled: visible,
      child: SingleChildScrollView(
        child: MutationRecoveryManager(
          backend: backend,
          seed: seed,
          ioBackend: backend,
          plugins: [_plugin(authorized: authorized)],
          registryRevision: revision ?? BigInt.one,
          ink: Colors.black,
          muted: Colors.grey,
          line: Colors.grey,
          radius: BorderRadius.circular(8),
        ),
      ),
    ),
  ),
);

bool _enabled(WidgetTester tester, String id) =>
    tester
        .widget<OutlinedButton>(find.byKey(ValueKey('mutation-recovery-$id')))
        .onPressed !=
    null;

Future<void> _tap(WidgetTester tester, String id) async {
  FocusManager.instance.primaryFocus?.unfocus();
  tester.testTextInput.hide();
  await tester.pumpAndSettle();
  final finder = find.byKey(ValueKey('mutation-recovery-$id'));
  await tester.ensureVisible(finder);
  await tester.pumpAndSettle();
  await tester.tap(finder);
  await tester.pump();
}

Future<void> _choosePackage(WidgetTester tester) async {
  final finder = find.byKey(const ValueKey('mutation-recovery-package'));
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await tester.pumpAndSettle();
  await tester.tap(find.text('Recovery test package').last);
  await tester.pumpAndSettle();
}

Future<void> _discover(WidgetTester tester, String subject) async {
  await _choosePackage(tester);
  await tester.enterText(
    find.byKey(const ValueKey('mutation-recovery-subject')),
    subject,
  );
  await tester.pump();
  await _tap(tester, 'discover');
}

void main() {
  testWidgets('workflow shortcuts prefill only and lock during recovery', (
    tester,
  ) async {
    final backend = _Backend()..queuedPages.add(_page([7], done: true));
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _choosePackage(tester);
    String subject() => tester
        .widget<TextField>(
          find.byKey(const ValueKey('mutation-recovery-subject')),
        )
        .controller!
        .text;
    await _tap(tester, 'native-scope');
    expect(subject(), mutationExecutionSubject);
    await _tap(tester, 'guest-scope');
    expect(subject(), guestMutationExecutionSubject);
    expect(backend.discoveryRequests, isEmpty);
    expect(backend.reconciliationRequests, isEmpty);
    await _tap(tester, 'discover');
    expect(
      backend.discoveryRequests.single.subject,
      guestMutationExecutionSubject,
    );
    expect(_enabled(tester, 'native-scope'), isFalse);
    expect(_enabled(tester, 'guest-scope'), isFalse);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('operation handoff only prefills idle recovery form', (
    tester,
  ) async {
    final backend = _Backend()..queuedPages.add(_page([7], done: true));
    final seed = MutationRecoverySeed(
      packageId: _plugin().id,
      packageDigest: _plugin().digest,
      subject: 'morrow.file-actions.v1',
      disposition: MutationDisposition.create,
    );
    await tester.pumpWidget(_screen(backend, seed: seed));
    await tester.pump();
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('mutation-recovery-subject')),
          )
          .controller!
          .text,
      seed.subject,
    );
    expect(_enabled(tester, 'discover'), isTrue);
    expect(backend.discoveryRequests, isEmpty);
    expect(backend.reconciliationRequests, isEmpty);
    await _tap(tester, 'discover');
    expect(backend.discoveryRequests.single.subject, seed.subject);
    final pendingSeed = MutationRecoverySeed(
      packageId: _plugin().id,
      packageDigest: _plugin().digest,
      subject: 'new.scope',
      disposition: MutationDisposition.create,
    );
    await tester.pumpWidget(_screen(backend, seed: pendingSeed));
    await tester.pump();
    expect(backend.discoveryRequests.single.subject, seed.subject);
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('mutation-recovery-subject')),
          )
          .controller!
          .text,
      seed.subject,
    );
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('title is localized across the nine supported locales', (
    tester,
  ) async {
    final backend = _Backend();
    for (final locale in [
      'zh',
      'en',
      'ru',
      'fr',
      'de',
      'es',
      'ja',
      'ko',
      'pt',
    ]) {
      await tester.pumpWidget(_screen(backend, locale: locale));
      await tester.pump();
      final l = L10n.forLocale(Locale(locale));
      expect(find.text(l.pluginsMutationTitle), findsOneWidget);
    }
    expect(backend.discoveryRequests, isEmpty);
    expect(backend.reads, 0);
    expect(backend.nexts, 0);
  });

  testWidgets('exact subject starts discovery without automatic read or next', (
    tester,
  ) async {
    final backend = _Backend()..queuedPages.add(_page([7]));
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, ' exact.subject ');
    expect(backend.discoveryRequests, hasLength(1));
    expect(backend.discoveryRequests.single.packageId, 'test.recovery-ui');
    expect(backend.discoveryRequests.single.subject, ' exact.subject ');
    expect(
      backend.discoveryRequests.single.disposition,
      MutationDisposition.create,
    );
    expect(backend.reads, 0);
    expect(backend.nexts, 0);
    expect(backend.reconciliationRequests, isEmpty);
    expect(_enabled(tester, 'read'), isTrue);
    await tester.pump(const Duration(seconds: 2));
    expect(backend.reads, 0);
    expect(backend.nexts, 0);
  });

  testWidgets('invalid subject never starts discovery', (tester) async {
    final backend = _Backend();
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'bad/path');
    expect(backend.discoveryRequests, isEmpty);
    expect(backend.reads, 0);
    expect(backend.nexts, 0);
  });

  testWidgets('continue uses delivered scope and checkpoint after cleanup', (
    tester,
  ) async {
    final backend = _Backend()
      ..queuedPages.addAll([
        _page([7]),
        _page([8], done: true),
      ]);
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, ' original.scope ');
    await _tap(tester, 'read');
    expect(backend.discoveryRequests, hasLength(1));
    await _tap(tester, 'stop');
    backend.joined();
    await _tap(tester, 'refresh');
    await _tap(tester, 'ack');
    await tester.enterText(
      find.byKey(const ValueKey('mutation-recovery-subject')),
      'changed.scope',
    );
    await tester.pump();
    expect(_enabled(tester, 'continue'), isTrue);
    await _tap(tester, 'continue');
    expect(backend.discoveryRequests, hasLength(2));
    final first = backend.discoveryRequests.first;
    final resumed = backend.discoveryRequests.last;
    expect(resumed.subject, first.subject);
    expect(resumed.subject, ' original.scope ');
    expect(resumed.packageId, first.packageId);
    expect(resumed.disposition, first.disposition);
    expect(resumed.checkpoint, _id(21));
    expect(resumed.submission, isNot(first.submission));
    expect(backend.reads, 1);
    expect(backend.nexts, 0);
  });

  testWidgets('320x640 can discover, read, clean up and continue', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(320, 640));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final backend = _Backend()
      ..queuedPages.addAll([
        _page([7]),
        _page([8], done: true),
      ]);
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'narrow.scope');
    await _tap(tester, 'read');
    await _tap(tester, 'stop');
    backend.joined();
    await _tap(tester, 'refresh');
    await _tap(tester, 'ack');
    expect(tester.takeException(), isNull);
    await _tap(tester, 'continue');
    expect(backend.discoveryRequests, hasLength(2));
    expect(backend.discoveryRequests.last.subject, 'narrow.scope');
    expect(backend.discoveryRequests.last.checkpoint, _id(21));
    expect(backend.reads, 1);
    expect(backend.nexts, 0);
    expect(tester.takeException(), isNull);
  });

  testWidgets('empty partial page is read once and next requires a tap', (
    tester,
  ) async {
    final backend = _Backend()
      ..queuedPages.addAll([
        _page([]),
        _page([4], done: true),
      ]);
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'empty.scope');
    await _tap(tester, 'read');
    expect(backend.reads, 1);
    expect(_enabled(tester, 'next'), isTrue);
    expect(backend.nexts, 0);
    await tester.pump(const Duration(seconds: 2));
    expect(backend.nexts, 0);
    await _tap(tester, 'next');
    expect(backend.nexts, 1);
    expect(backend.reads, 1);
    await _tap(tester, 'read');
    expect(backend.reads, 2);
    expect(_enabled(tester, 'next'), isFalse);
  });

  testWidgets('lost consumed read blocks replay and next after refresh', (
    tester,
  ) async {
    final backend = _Backend()
      ..queuedPages.add(_page([1]))
      ..loseRead = true;
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'lost.scope');
    await _tap(tester, 'read');
    await _tap(tester, 'refresh');
    expect(_enabled(tester, 'read'), isFalse);
    expect(_enabled(tester, 'next'), isFalse);
    expect(backend.reads, 1);
    expect(backend.nexts, 0);
    expect(backend.reconciliationRequests, isEmpty);
    expect(_enabled(tester, 'stop'), isTrue);
  });

  testWidgets('selected plan reconciles only after explicit cleanup and ack', (
    tester,
  ) async {
    final backend = _Backend()..queuedPages.add(_page([7]));
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'original.scope');
    await _tap(tester, 'read');
    final tile = find.byKey(const ValueKey('mutation-recovery-plan-0'));
    await tester.ensureVisible(tile);
    await tester.tap(tile);
    await tester.pump();
    expect(
      find.descendant(
        of: tile,
        matching: find.byIcon(Icons.check_circle_outline),
      ),
      findsOneWidget,
    );
    expect(_enabled(tester, 'reconcile'), isFalse);
    await _tap(tester, 'release');
    expect(_enabled(tester, 'stop'), isTrue);
    expect(backend.reconciliationRequests, isEmpty);
    await _tap(tester, 'read');
    await _tap(tester, 'stop');
    backend.joined();
    await _tap(tester, 'refresh');
    expect(_enabled(tester, 'ack'), isTrue);
    await _tap(tester, 'ack');
    expect(_enabled(tester, 'reconcile'), isTrue);
    await _tap(tester, 'reconcile');
    expect(backend.reconciliationRequests, hasLength(1));
    expect(backend.reconciliationRequests.single.plan, [7]);
    expect(backend.reconciliationRequests.single.packageId, 'test.recovery-ui');
    expect(
      backend.reconciliationRequests.single.submission,
      isNot(backend.discoveryRequests.single.submission),
    );
    await _tap(tester, 'read');
    final l = L10n.forLocale(const Locale('en'));
    expect(find.text(l.pluginsMutationUnknown), findsWidgets);
    expect(find.text(l.pluginsMutationSucceeded), findsNothing);
  });

  testWidgets('remount retains page; switching backend isolates it', (
    tester,
  ) async {
    final first = _Backend()..queuedPages.add(_page([7]));
    await tester.pumpWidget(_screen(first));
    await tester.pump();
    await _discover(tester, 'mounted.scope');
    await _tap(tester, 'read');
    expect(
      find.byKey(const ValueKey('mutation-recovery-plan-0')),
      findsOneWidget,
    );
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpWidget(_screen(first));
    await tester.pump();
    expect(
      find.byKey(const ValueKey('mutation-recovery-plan-0')),
      findsOneWidget,
    );
    expect(first.discoveryRequests, hasLength(1));

    final second = _Backend();
    await tester.pumpWidget(_screen(second));
    await tester.pump();
    expect(
      find.byKey(const ValueKey('mutation-recovery-plan-0')),
      findsNothing,
    );
    expect(second.discoveryRequests, isEmpty);
    expect(first.discoveryRequests, hasLength(1));
  });

  testWidgets('catalog revocation disables new start but keeps cleanup', (
    tester,
  ) async {
    final backend = _Backend()..queuedPages.add(_page([7]));
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'revoked.scope');
    await _tap(tester, 'read');
    final tile = find.byKey(const ValueKey('mutation-recovery-plan-0'));
    await tester.ensureVisible(tile);
    await tester.tap(tile);
    await tester.pump();
    await tester.pumpWidget(_screen(backend, authorized: false));
    await tester.pump();
    expect(_enabled(tester, 'discover'), isFalse);
    expect(_enabled(tester, 'release'), isTrue);
    await _tap(tester, 'release');
    await _tap(tester, 'read');
    await _tap(tester, 'stop');
    backend.joined();
    await _tap(tester, 'refresh');
    expect(_enabled(tester, 'ack'), isTrue);
    await _tap(tester, 'ack');
    expect(_enabled(tester, 'reconcile'), isFalse);
    expect(_enabled(tester, 'continue'), isFalse);
  });

  testWidgets('hidden view stops polling; return only refreshes status', (
    tester,
  ) async {
    final backend = _Backend()..queuedPages.add(_page([7]));
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    await _discover(tester, 'poll.scope');
    final starts = backend.discoveryRequests.length;
    await tester.pump(const Duration(seconds: 2));
    final visibleStatuses = backend.statuses;
    expect(visibleStatuses, greaterThan(0));
    await tester.pumpWidget(_screen(backend, visible: false));
    await tester.pump();
    final hiddenStatuses = backend.statuses;
    await tester.pump(const Duration(seconds: 3));
    expect(backend.statuses, hiddenStatuses);
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    expect(backend.statuses, greaterThan(hiddenStatuses));
    expect(backend.discoveryRequests, hasLength(starts));
    expect(backend.reads, 0);
    expect(backend.nexts, 0);
  });
}
