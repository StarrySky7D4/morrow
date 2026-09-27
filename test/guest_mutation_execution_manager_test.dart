import 'dart:async';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_manager.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_session.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

Uint8List _id(int value) => Uint8List.fromList(List.filled(32, value));
final _plan = Uint8List.fromList([9, 8, 7]);
final _planHash = Uint8List.fromList(sha256.convert(_plan).bytes);

MutationResult _owner(MutationResultKind kind, {Uint8List? plan}) =>
    MutationResult(
      kind: kind,
      reference: kind == MutationResultKind.selected ? _id(4) : null,
      plan: plan,
      stagedBytes: BigInt.zero,
      durableContent: false,
      phase: MutationPhase.none,
      operationId: null,
      effect: MutationEffect.unspecified,
      osCode: null,
      scanned: 0,
      done: false,
    );

class _Backend implements GuestMutationBackend, WorkbenchIoTaskControl {
  Uint8List? key, startSubmission;
  GuestMutationStartRequest? request;
  GuestMutationCommand? lastCommand;
  GuestMutationReceipt? receipt;
  GuestMutationResult? result;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  GuestMutationDelivery delivery = GuestMutationDelivery.ready;
  GuestMutationCommandKind kind = GuestMutationCommandKind.select;
  BigInt commandId = BigInt.one, staged = BigInt.zero;
  bool approvalDelivered = false, permitDelivered = false;
  bool terminal = false, reconcileRequired = false, durable = false;
  int starts = 0, executes = 0, chunks = 0, statuses = 0;
  ApprovedGuestBudget? approvedBudget;
  Uint8List? body;
  BigInt? plannedLength;
  Uint8List? plannedHash;
  Future<void> Function()? beforeStatus;

  IoTaskSnapshot _snapshot() => IoTaskSnapshot(
    key: key,
    submission: startSubmission,
    storage: storage,
    delivery: switch (delivery) {
      GuestMutationDelivery.pending => IoDeliveryPhase.pending,
      GuestMutationDelivery.ready => IoDeliveryPhase.ready,
      GuestMutationDelivery.consumed => IoDeliveryPhase.consumed,
      GuestMutationDelivery.unavailable => IoDeliveryPhase.unavailable,
    },
    exit: exit,
  );

  GuestMutationState _state() => GuestMutationState(
    commandId: commandId,
    kind: kind,
    delivery: delivery,
    selected: key != null,
    reference: key == null ? null : _id(4),
    reviewedPlanSha256: kind.index >= GuestMutationCommandKind.buildPlan.index
        ? _planHash
        : null,
    approvalDelivered: approvalDelivered,
    permitDelivered: permitDelivered,
    stagedBytes: staged,
    durableContent: durable,
    effectAttempted: executes != 0,
    reconcileRequired: reconcileRequired,
    terminal: terminal,
  );

  GuestMutationTaskReply _reply({bool status = false}) =>
      GuestMutationTaskReply(
        io: _snapshot(),
        state: _state(),
        commandId: status ? BigInt.zero : commandId,
        receipt: status ? null : receipt,
      );

  GuestMutationFrame _frame(GuestMutationCommand command) {
    final frameKind = GuestMutationValidation.frameKind(
      command.kind,
      command.disposition,
    );
    final phase = command.kind == GuestMutationCommandKind.execute
        ? GuestMutationFramePhase.observed
        : GuestMutationFramePhase.prepared;
    return GuestMutationFrame(
      callId: commandId,
      reference: _id(5),
      submission: receipt!.submission,
      operationId: command.operationId!,
      kind: frameKind,
      status: GuestMutationFrameStatus.completed,
      phase: phase,
      effect: command.kind == GuestMutationCommandKind.execute
          ? GuestMutationFrameEffect.osSucceeded
          : GuestMutationFrameEffect.unspecified,
      stagedBytes: staged,
      durableContent: durable,
      encoded: Uint8List.fromList([1]),
    );
  }

  GuestMutationReceipt _receipt(
    Uint8List token,
    GuestMutationCommand? command,
  ) => GuestMutationReceipt(
    key: key!,
    submission: token,
    commandId: commandId,
    kind: command?.kind ?? GuestMutationCommandKind.select,
    operationId: command?.operationId,
    disposition: command?.disposition,
    expectedGuestReference: command?.expectedGuestReference,
    expectedChunkEnd: command?.kind == GuestMutationCommandKind.chunk
        ? command!.offset! + BigInt.from(command.bytes!.length)
        : null,
  );

  @override
  Future<GuestMutationTaskReply> start(GuestMutationStartRequest value) async {
    starts++;
    request = value;
    approvedBudget = value.approvedBudget;
    key = _id(3);
    startSubmission = value.selection.submission;
    storage = IoStoragePhase.running;
    kind = GuestMutationCommandKind.select;
    commandId = BigInt.one;
    delivery = GuestMutationDelivery.ready;
    receipt = _receipt(value.selection.submission, null);
    result = GuestMutationResult(
      kind: GuestMutationResultKind.owner,
      owner: _owner(MutationResultKind.selected),
    );
    return _reply();
  }

  @override
  Future<GuestMutationTaskReply> submit(
    Uint8List taskKey,
    Uint8List token,
    GuestMutationCommand command,
  ) async {
    lastCommand = command;
    commandId += BigInt.one;
    kind = command.kind;
    delivery = GuestMutationDelivery.ready;
    receipt = _receipt(token, command);
    switch (command.kind) {
      case GuestMutationCommandKind.buildPlan:
        plannedLength = command.contentLength;
        plannedHash = command.contentSha256;
        result = GuestMutationResult(
          kind: GuestMutationResultKind.owner,
          owner: _owner(MutationResultKind.planned, plan: _plan),
        );
      case GuestMutationCommandKind.prepare:
        approvalDelivered = true;
        result = GuestMutationResult(
          kind: GuestMutationResultKind.frame,
          frame: _frame(command),
        );
      case GuestMutationCommandKind.chunk:
        chunks++;
        staged = command.offset! + BigInt.from(command.bytes!.length);
        body = Uint8List.fromList(command.bytes!);
        result = GuestMutationResult(
          kind: GuestMutationResultKind.frame,
          frame: _frame(command),
        );
      case GuestMutationCommandKind.commitContent:
        durable = true;
        result = GuestMutationResult(
          kind: GuestMutationResultKind.frame,
          frame: _frame(command),
        );
      case GuestMutationCommandKind.execute:
        executes++;
        permitDelivered = true;
        terminal = true;
        result = GuestMutationResult(
          kind: GuestMutationResultKind.frame,
          frame: _frame(command),
        );
      default:
        throw UnimplementedError(command.kind.name);
    }
    return _reply();
  }

  @override
  Future<GuestMutationTaskReply> status(Uint8List taskKey) async {
    statuses++;
    await beforeStatus?.call();
    return _reply(status: true);
  }

  @override
  Future<GuestMutationTaskRead> read(GuestMutationReceipt value) async {
    delivery = GuestMutationDelivery.consumed;
    final current = result;
    result = null;
    return GuestMutationTaskRead(reply: _reply(), result: current);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return _snapshot();
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    storage = IoStoragePhase.stopping;
    return _snapshot();
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    expect(taskKey, key);
    key = null;
    storage = IoStoragePhase.local;
    exit = null;
    return _snapshot();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}

class _HangingContent extends XFile {
  _HangingContent() : super(r'C:\virtual\hanging.bin') {
    controller.onCancel = () => cancelled = true;
  }
  final controller = StreamController<Uint8List>();
  bool cancelled = false;

  @override
  Future<int> length() async => 1;

  @override
  Stream<Uint8List> openRead([int? start, int? end]) => controller.stream;
}

PluginLibraryEntry _plugin({bool authorized = true, bool budgeted = true}) =>
    PluginLibraryEntry(
      id: 'test.guest',
      name: 'Guest package',
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
      declaredIo: const ['file-create', 'file-delete'],
      approvedIo: authorized ? const ['file-create', 'file-delete'] : const [],
      mutationSupported: true,
      mutationBudget: budgeted
          ? PluginMutationBudget(
              maxJobBytes: BigInt.from(32 * 1024 * 1024),
              maxBytes: BigInt.from(256 * 1024 * 1024),
            )
          : null,
    );

Widget _screen(
  _Backend backend, {
  bool authorized = true,
  bool budgeted = true,
  BigInt? revision,
  Future<String?> Function()? pickDirectory,
  Future<XFile?> Function()? pickContent,
  VoidCallback? onOpenRecovery,
}) => MaterialApp(
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: GuestMutationExecutionManager(
        backend: backend,
        ioBackend: backend,
        plugins: [_plugin(authorized: authorized, budgeted: budgeted)],
        registryRevision: revision ?? BigInt.one,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(8),
        pickDirectory: pickDirectory,
        pickContent: pickContent,
        onOpenRecovery: onOpenRecovery,
      ),
    ),
  ),
);

Finder _key(String id) => find.byKey(ValueKey('guest-mutation-$id'));
bool _enabled(WidgetTester tester, String id) =>
    _key(id).evaluate().isNotEmpty &&
    tester.widget<OutlinedButton>(_key(id)).onPressed != null;

Future<void> _tap(WidgetTester tester, String id) async {
  FocusManager.instance.primaryFocus?.unfocus();
  tester.testTextInput.hide();
  await tester.pump();
  await tester.ensureVisible(_key(id));
  await tester.pump();
  await tester.tap(_key(id));
  await tester.pump();
}

Future<void> _waitForDialog(WidgetTester tester, String step) async {
  for (var i = 0; i < 40 && _key('confirm-$step').evaluate().isEmpty; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump(const Duration(milliseconds: 50));
  }
  expect(_key('confirm-$step'), findsOneWidget);
}

Future<void> _create(WidgetTester tester, {bool budgeted = true}) async {
  await tester.ensureVisible(_key('package'));
  await tester.pump();
  await tester.tap(_key('package'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Guest package').last);
  await tester.pumpAndSettle();
  if (budgeted) {
    await tester.enterText(_key('job-budget'), '${32 * 1024 * 1024}');
    await tester.enterText(_key('total-budget'), '${256 * 1024 * 1024}');
  }
  await _tap(tester, 'pick-target');
  await tester.enterText(_key('file-name'), 'created.bin');
  await tester.pump();
}

void main() {
  for (final stage in ['prepare', 'execute']) {
    for (final duringDialog in [true, false]) {
      testWidgets(
        '$stage expiry ${duringDialog ? 'while dialog is open' : 'during final status check'} blocks dispatch and permits cleanup',
        (tester) async {
          var now = Duration.zero;
          final backend = _Backend();
          final session = GuestMutationExecutionSession.forBackend(
            backend,
            backend,
            monotonicNow: () => now,
          );
          await tester.pumpWidget(
            _screen(
              backend,
              pickDirectory: () async => r'C:\approved',
              pickContent: () async => XFile.fromData(
                Uint8List.fromList([7, 8, 9]),
                name: 'body.bin',
              ),
            ),
          );
          await tester.pump();
          await _create(tester);
          await _tap(tester, 'pick-content');
          await _tap(tester, 'draft');
          await _waitForDialog(tester, 'draft');
          await tester.tap(_key('confirm-draft'));
          await tester.pumpAndSettle();
          if (stage == 'execute') {
            await _tap(tester, 'prepare');
            await tester.tap(_key('confirm-prepare'));
            await tester.pumpAndSettle();
            expect(backend.chunks, 1);
            expect(backend.durable, isTrue);
          }
          final before = backend.commandId;
          await _tap(tester, stage);
          if (duringDialog) {
            now = const Duration(seconds: 30);
            await tester.pump(const Duration(seconds: 1));
            expect(
              tester.widget<FilledButton>(_key('confirm-$stage')).onPressed,
              isNull,
            );
            expect(find.textContaining('local approval window'), findsWidgets);
            await tester.tap(_key('confirm-cancel'));
          } else {
            // The button was enabled when clicked; expiry while observing the
            // owner must still prevent a command, including a new Prepare.
            backend.beforeStatus = () async {
              now = const Duration(seconds: 30);
            };
            await tester.tap(_key('confirm-$stage'));
          }
          await tester.pumpAndSettle();
          expect(session.authorizationExpired, isTrue);
          expect(backend.commandId, before);
          expect(backend.executes, 0);
          expect(_enabled(tester, 'prepare'), isFalse);
          expect(_enabled(tester, 'execute'), isFalse);
          expect(_enabled(tester, 'release'), isFalse);
          expect(_enabled(tester, 'stop'), isTrue);
          final observations = backend.statuses;
          await tester.pump(const Duration(seconds: 1));
          await tester.pump();
          expect(
            backend.statuses,
            greaterThan(observations),
            reason: 'Closing a review must resume display polling',
          );
          await _tap(tester, 'stop');
          await tester.pumpAndSettle();
          expect(
            _enabled(tester, 'ack'),
            isFalse,
            reason: 'Stop is not exit proof',
          );
          backend.storage = IoStoragePhase.reclaimed;
          backend.exit = const IoTaskExit(
            execution: IoJobError.none,
            disconnect: IoJobError.none,
            maintenance: IoJobError.none,
          );
          await _tap(tester, 'refresh');
          await tester.pumpAndSettle();
          expect(_enabled(tester, 'ack'), isTrue);
          await _tap(tester, 'ack');
          await tester.pumpAndSettle();
          expect(session.canStart, isTrue);
          expect(session.authorizationExpired, isFalse);
          expect(backend.starts, 1);
          expect(backend.executes, 0);
        },
      );
    }
  }
  testWidgets(
    'budget is explicitly approved and two separate confirmations gate Execute',
    (tester) async {
      final backend = _Backend();
      final content = Uint8List.fromList([4, 5, 6]);
      await tester.pumpWidget(
        _screen(
          backend,
          pickDirectory: () async => r'C:\approved',
          pickContent: () async => XFile.fromData(content, name: 'source.bin'),
        ),
      );
      await tester.pump();
      await _create(tester);
      await _tap(tester, 'pick-content');
      await _tap(tester, 'draft');
      await _waitForDialog(tester, 'draft');
      expect(backend.starts, 0);
      expect(find.textContaining('Create'), findsWidgets);
      await tester.tap(_key('confirm-cancel'));
      await tester.pumpAndSettle();
      expect(backend.starts, 0);
      await _tap(tester, 'draft');
      await _waitForDialog(tester, 'draft');
      await tester.tap(_key('confirm-draft'));
      await tester.pumpAndSettle();
      expect(backend.starts, 1);
      expect(backend.approvedBudget?.maxBytes, BigInt.from(256 * 1024 * 1024));
      expect(backend.plannedLength, BigInt.from(content.length));
      expect(backend.plannedHash, sha256.convert(content).bytes);
      expect(backend.executes, 0);
      expect(_enabled(tester, 'prepare'), isTrue);
      await _tap(tester, 'prepare');
      expect(backend.lastCommand?.kind, GuestMutationCommandKind.buildPlan);
      await tester.tap(_key('confirm-cancel'));
      await tester.pumpAndSettle();
      expect(_enabled(tester, 'prepare'), isTrue);
      await _tap(tester, 'prepare');
      await tester.tap(_key('confirm-prepare'));
      await tester.pumpAndSettle();
      expect(backend.lastCommand?.kind, GuestMutationCommandKind.commitContent);
      expect(backend.chunks, 1);
      expect(backend.body, content);
      expect(backend.executes, 0);
      await _tap(tester, 'execute');
      await tester.tap(_key('confirm-cancel'));
      await tester.pumpAndSettle();
      expect(backend.executes, 0);
      await _tap(tester, 'execute');
      await tester.tap(_key('confirm-execute'));
      await tester.pumpAndSettle();
      expect(backend.executes, 1);
      expect(find.textContaining('Plugin execution result'), findsWidgets);
    },
  );

  testWidgets(
    'invalid approved budget blocks admission; ordinary guest has no extension',
    (tester) async {
      final backend = _Backend();
      await tester.pumpWidget(
        _screen(backend, pickDirectory: () async => r'C:\approved'),
      );
      await tester.pump();
      await _create(tester);
      await tester.enterText(_key('job-budget'), '${33 * 1024 * 1024}');
      await _tap(tester, 'draft');
      expect(backend.starts, 0);
      expect(find.textContaining('budget'), findsWidgets);
      final ordinary = _Backend();
      await tester.pumpWidget(
        _screen(
          ordinary,
          budgeted: false,
          pickDirectory: () async => r'C:\approved',
        ),
      );
      await tester.pump();
      await _create(tester, budgeted: false);
      expect(_key('job-budget'), findsNothing);
      await _tap(tester, 'draft');
      await tester.tap(_key('confirm-draft'));
      await tester.pumpAndSettle();
      expect(ordinary.request?.approvedBudget, isNull);
    },
  );

  testWidgets(
    'catalog revocation while Execute dialog is open prevents dispatch',
    (tester) async {
      final backend = _Backend();
      await tester.pumpWidget(
        _screen(backend, pickDirectory: () async => r'C:\approved'),
      );
      await tester.pump();
      await _create(tester);
      await _tap(tester, 'draft');
      await tester.tap(_key('confirm-draft'));
      await tester.pumpAndSettle();
      await _tap(tester, 'prepare');
      await tester.tap(_key('confirm-prepare'));
      await tester.pumpAndSettle();
      expect(_enabled(tester, 'execute'), isTrue);
      await _tap(tester, 'execute');
      unawaited(
        showDialog<void>(
          context: tester.element(_key('confirm-execute')),
          builder: (_) => const AlertDialog(title: Text('Unrelated dialog')),
        ),
      );
      await tester.pumpAndSettle();
      await tester.pumpWidget(
        _screen(
          backend,
          authorized: false,
          revision: BigInt.two,
          pickDirectory: () async => r'C:\approved',
        ),
      );
      await tester.pump();
      expect(_key('confirm-execute'), findsNothing);
      expect(find.text('Unrelated dialog'), findsOneWidget);
      Navigator.of(tester.element(find.text('Unrelated dialog'))).pop();
      await tester.pumpAndSettle();
      expect(_enabled(tester, 'execute'), isFalse);
      expect(backend.executes, 0);
    },
  );

  testWidgets('cancel local content review without admitting guest task', (
    tester,
  ) async {
    final backend = _Backend(), source = _HangingContent();
    await tester.pumpWidget(
      _screen(
        backend,
        pickDirectory: () async => r'C:\approved',
        pickContent: () async => source,
      ),
    );
    await tester.pump();
    await _create(tester);
    await _tap(tester, 'pick-content');
    await _tap(tester, 'draft');
    for (var i = 0; i < 20 && !source.controller.hasListener; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(source.controller.hasListener, isTrue);
    expect(_key('cancel-review'), findsOneWidget);
    await _tap(tester, 'cancel-review');
    expect(source.cancelled, isTrue);
    expect(backend.starts, 0);
    expect(_enabled(tester, 'draft'), isTrue);
  });

  testWidgets('recovery handoff waits for actual exit and ACK', (tester) async {
    final backend = _Backend();
    var opens = 0;
    await tester.pumpWidget(
      _screen(
        backend,
        pickDirectory: () async => r'C:\approved',
        onOpenRecovery: () => opens++,
      ),
    );
    await tester.pump();
    await _create(tester);
    await _tap(tester, 'draft');
    await tester.tap(_key('confirm-draft'));
    await tester.pumpAndSettle();
    expect(backend.starts, 1);
    expect(_key('recovery'), findsNothing);
    await _tap(tester, 'stop');
    expect(_enabled(tester, 'ack'), isFalse);
    expect(_key('recovery'), findsNothing);
    backend.exit = const IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: IoJobError.none,
    );
    backend.storage = IoStoragePhase.reclaimed;
    await _tap(tester, 'refresh');
    expect(_enabled(tester, 'ack'), isTrue);
    await _tap(tester, 'ack');
    expect(_enabled(tester, 'recovery'), isTrue);
    await _tap(tester, 'recovery');
    expect(opens, 1);
    expect(backend.starts, 1); // Handoff never starts a new mutation.
  });
}
