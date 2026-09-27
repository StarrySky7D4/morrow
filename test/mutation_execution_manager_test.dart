import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/mutation_execution_manager.dart';
import 'package:morrow_studio/plugins/mutation_execution_session.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

Uint8List _id(int byte) => Uint8List.fromList(List.filled(32, byte));

MutationResult _result(
  MutationResultKind kind, {
  Uint8List? plan,
  bool durable = false,
  BigInt? staged,
  MutationPhase phase = MutationPhase.none,
  MutationEffect effect = MutationEffect.unspecified,
  String? operationId,
}) => MutationResult(
  kind: kind,
  plan: plan,
  stagedBytes: staged ?? BigInt.zero,
  durableContent: durable,
  phase: phase,
  operationId: operationId,
  effect: effect,
  osCode: null,
  scanned: 0,
  done: false,
);

class _Backend implements MutationTaskBackend, WorkbenchIoTaskControl {
  Uint8List? key, submission;
  IoStoragePhase storage = IoStoragePhase.local;
  IoTaskExit? exit;
  MutationCommandKind kind = MutationCommandKind.select;
  MutationDelivery delivery = MutationDelivery.ready;
  BigInt command = BigInt.one;
  MutationResult? pending;
  int starts = 0, executes = 0, statuses = 0, reads = 0, acks = 0;
  int chunks = 0, releases = 0;
  BigInt? contentLength;
  String? operationId;
  Uint8List? contentHash;
  Uint8List? approvalHash;
  Uint8List? stagedContent;
  MutationStartRequest? request;
  MutationPhase executedPhase = MutationPhase.observed;
  MutationEffect executedEffect = MutationEffect.osSucceeded;
  bool badPrepared = false, failStatus = false;

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
      selected: true,
      reconcileRequired: false,
      terminal: false,
    ),
    commandId: status ? BigInt.zero : command,
  );

  MutationTaskReply _command(MutationCommandKind next, MutationResult result) {
    command += BigInt.one;
    kind = next;
    pending = result;
    delivery = MutationDelivery.ready;
    return _reply();
  }

  @override
  Future<MutationTaskReply> startSelected(MutationStartRequest value) async {
    starts++;
    request = value;
    approvalHash = value.approvalSha256;
    key = _id(3);
    submission = value.submission;
    storage = IoStoragePhase.running;
    exit = null;
    command = BigInt.one;
    kind = MutationCommandKind.select;
    delivery = MutationDelivery.ready;
    pending = _result(MutationResultKind.selected);
    return _reply();
  }

  @override
  Future<MutationTaskReply> submitBuildPlan(
    Uint8List taskKey,
    Uint8List token, {
    required String operationId,
    required BigInt contentLength,
    Uint8List? contentSha256,
  }) async {
    this.operationId = operationId;
    this.contentLength = contentLength;
    contentHash = contentSha256;
    return _command(
      MutationCommandKind.buildPlan,
      _result(MutationResultKind.planned, plan: Uint8List.fromList([9])),
    );
  }

  @override
  Future<MutationTaskReply> submitPrepare(
    Uint8List taskKey,
    Uint8List token,
    Uint8List plan,
  ) async => _command(
    MutationCommandKind.prepare,
    _result(
      MutationResultKind.prepared,
      phase: MutationPhase.prepared,
      operationId: badPrepared ? 'foreign-operation' : operationId,
    ),
  );

  @override
  Future<MutationTaskReply> submitChunk(
    Uint8List taskKey,
    Uint8List token,
    BigInt offset,
    Uint8List bytes,
  ) async {
    chunks++;
    stagedContent = Uint8List.fromList(bytes);
    return _command(
      MutationCommandKind.chunk,
      _result(
        MutationResultKind.staged,
        staged: offset + BigInt.from(bytes.length),
      ),
    );
  }

  @override
  Future<MutationTaskReply> submitCommitContent(
    Uint8List taskKey,
    Uint8List token,
  ) async => _command(
    MutationCommandKind.commitContent,
    _result(MutationResultKind.staged, durable: true, staged: contentLength),
  );

  @override
  Future<MutationTaskReply> submitExecute(
    Uint8List taskKey,
    Uint8List token,
  ) async {
    executes++;
    return _command(
      MutationCommandKind.execute,
      _result(
        request!.disposition == MutationDisposition.delete
            ? MutationResultKind.deleted
            : MutationResultKind.created,
        phase: executedPhase,
        effect: executedEffect,
        operationId: operationId,
      ),
    );
  }

  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List taskKey,
    Uint8List token,
  ) async {
    releases++;
    return _command(
      MutationCommandKind.release,
      _result(MutationResultKind.released),
    );
  }

  @override
  Future<MutationTaskReply> status(Uint8List taskKey) async {
    statuses++;
    if (failStatus) throw StateError('status diagnostics');
    return _reply(status: true);
  }

  @override
  Future<MutationTaskRead> read(Uint8List taskKey, BigInt commandId) async {
    reads++;
    delivery = MutationDelivery.consumed;
    final result = pending;
    pending = null;
    return MutationTaskRead(reply: _reply(), result: result);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return _snapshot();
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List taskKey) async {
    storage = IoStoragePhase.stopping;
    return _snapshot();
  }

  void joined() {
    storage = IoStoragePhase.reclaimed;
    exit = const IoTaskExit(
      execution: IoJobError.none,
      disconnect: IoJobError.none,
      maintenance: IoJobError.none,
    );
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List taskKey) async {
    storage = IoStoragePhase.reclaimed;
    return _snapshot();
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List taskKey) async {
    acks++;
    key = null;
    submission = null;
    exit = null;
    storage = IoStoragePhase.local;
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

PluginLibraryEntry _plugin({
  bool authorized = true,
  bool budgetedGuest = false,
}) => PluginLibraryEntry(
  id: 'test.mutation-execution',
  name: 'Authorized file package',
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
  mutationSupported: budgetedGuest,
  mutationBudget: budgetedGuest
      ? PluginMutationBudget(
          maxJobBytes: BigInt.from(32 * 1024 * 1024),
          maxBytes: BigInt.from(256 * 1024 * 1024),
        )
      : null,
);

Widget _screen(
  _Backend backend, {
  bool authorized = true,
  bool budgetedGuest = false,
  bool visible = true,
  Future<String?> Function()? pickDirectory,
  Future<XFile?> Function()? pickFile,
  Future<XFile?> Function()? pickContent,
  VoidCallback? onOpenRecovery,
}) => MaterialApp(
  locale: const Locale('en'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: TickerMode(
      enabled: visible,
      child: SingleChildScrollView(
        child: MutationExecutionManager(
          backend: backend,
          ioBackend: backend,
          plugins: [
            _plugin(authorized: authorized, budgetedGuest: budgetedGuest),
          ],
          registryRevision: BigInt.one,
          ink: Colors.black,
          muted: Colors.grey,
          line: Colors.grey,
          radius: BorderRadius.circular(8),
          pickDirectory: pickDirectory,
          pickFile: pickFile,
          pickContent: pickContent,
          onOpenRecovery: onOpenRecovery,
        ),
      ),
    ),
  ),
);

Finder _key(String id) => find.byKey(ValueKey('mutation-execution-$id'));

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

Future<void> _package(WidgetTester tester) async {
  final field = find.byKey(const ValueKey('mutation-execution-package'));
  await tester.tap(field);
  await tester.pumpAndSettle();
  await tester.tap(find.text('Authorized file package').last);
  await tester.pumpAndSettle();
}

Future<void> _create(WidgetTester tester, {String name = 'created.bin'}) async {
  await _package(tester);
  await _tap(tester, 'pick-target');
  await tester.enterText(
    find.byKey(const ValueKey('mutation-execution-file-name')),
    name,
  );
  await tester.pump();
}

Future<void> _prepare(WidgetTester tester) async {
  await _tap(tester, 'review');
  expect(_key('confirm-prepare'), findsOneWidget);
  await tester.tap(_key('confirm-prepare'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('budgeted guests are not offered on the ordinary native route', (
    tester,
  ) async {
    final backend = _Backend();
    await tester.pumpWidget(_screen(backend, budgetedGuest: true));
    await tester.pump();
    final choices = tester.widget<DropdownButton<String>>(_key('package'));
    expect(choices.items, isEmpty);
    expect(_enabled(tester, 'review'), isFalse);
    expect(backend.starts, 0);
  });
  testWidgets('review and both confirmations gate Create execution', (
    tester,
  ) async {
    final backend = _Backend();
    await tester.pumpWidget(
      _screen(backend, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    await _create(tester);
    await _tap(tester, 'review');
    expect(backend.starts, 0);
    expect(backend.executes, 0);
    expect(find.textContaining('created.bin'), findsWidgets);
    await tester.tap(_key('confirm-cancel'));
    await tester.pumpAndSettle();
    expect(backend.starts, 0);
    await _prepare(tester);
    expect(backend.starts, 1);
    expect(backend.executes, 0);
    expect(backend.request!.subject, mutationExecutionSubject);
    expect(_enabled(tester, 'execute'), isTrue);
    final session = MutationExecutionSession.forBackend(backend, backend);
    expect(session.ready, isTrue);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpWidget(
      _screen(backend, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    expect(backend.starts, 1);
    expect(backend.executes, 0);
    await _tap(tester, 'execute');
    expect(backend.executes, 0);
    await tester.tap(_key('confirm-cancel'));
    await tester.pumpAndSettle();
    expect(backend.executes, 0);
    await _tap(tester, 'execute');
    await tester.tap(_key('confirm-execute'));
    await tester.pumpAndSettle();
    expect(backend.executes, 1);
  });

  testWidgets('Delete requires distinct explicit destructive confirmation', (
    tester,
  ) async {
    final backend = _Backend()
      ..executedPhase = MutationPhase.outcomeUnknown
      ..executedEffect = MutationEffect.unspecified;
    await tester.pumpWidget(
      _screen(backend, pickFile: () async => XFile(r'C:\approved\delete.bin')),
    );
    await tester.pump();
    await tester.tap(
      find.byKey(const ValueKey('mutation-execution-disposition')),
    );
    await tester.pumpAndSettle();
    final l = L10n.forLocale(const Locale('en'));
    await tester.tap(find.text(l.pluginsIoFileDelete).last);
    await tester.pumpAndSettle();
    await _package(tester);
    await _tap(tester, 'pick-target');
    await _prepare(tester);
    expect(backend.request!.disposition, MutationDisposition.delete);
    expect(backend.chunks, 0);
    expect(backend.executes, 0);
    await _tap(tester, 'execute');
    expect(find.text(l.pluginsFileMutationExecuteDeleteConfirm), findsWidgets);
    expect(backend.executes, 0);
    await tester.tap(_key('confirm-execute'));
    await tester.pumpAndSettle();
    expect(backend.executes, 1);
    expect(find.text(l.pluginsMutationUnknown), findsWidgets);
    expect(find.text(l.pluginsMutationSucceeded), findsNothing);
  });

  testWidgets('picked Create bytes and approval match actual bounded content', (
    tester,
  ) async {
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
    await tester.pumpAndSettle();
    expect(_enabled(tester, 'review'), isTrue);
    await _tap(tester, 'review');
    for (var i = 0; i < 30 && _key('confirm-prepare').evaluate().isEmpty; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(_key('confirm-prepare'), findsOneWidget);
    expect(find.textContaining('3'), findsWidgets);
    expect(
      find.textContaining(sha256.convert(content).toString()),
      findsWidgets,
    );
    await tester.tap(_key('confirm-prepare'));
    await tester.pumpAndSettle();
    expect(backend.contentLength, BigInt.from(3));
    expect(backend.contentHash, sha256.convert(content).bytes);
    expect(backend.stagedContent, content);
    expect(
      backend.approvalHash,
      sha256
          .convert(
            utf8.encode(
              jsonEncode([
                'morrow.file-actions.approval.v1',
                'test.mutation-execution',
                '02' * 32,
                '1',
                'create',
                r'C:\approved',
                'created.bin',
                backend.operationId,
                3,
                sha256.convert(content).toString(),
              ]),
            ),
          )
          .bytes,
    );
    expect(backend.executes, 0);
  });

  testWidgets('cancelled and late picker never submit an operation', (
    tester,
  ) async {
    final backend = _Backend();
    final late = Completer<String?>();
    var calls = 0;
    await tester.pumpWidget(
      _screen(
        backend,
        pickDirectory: () {
          calls++;
          return calls == 1 ? Future<String?>.value(null) : late.future;
        },
      ),
    );
    await tester.pump();
    await _package(tester);
    await _tap(tester, 'pick-target');
    await _tap(tester, 'review');
    expect(backend.starts, 0);
    await _tap(tester, 'pick-target');
    await tester.pumpWidget(_screen(backend, authorized: false));
    late.complete(r'C:\late');
    await tester.pump();
    expect(backend.starts, 0);
    expect(_enabled(tester, 'review'), isFalse);
    expect(_enabled(tester, 'pick-target'), isFalse);
  });

  testWidgets('stalled source read can be cancelled before admission', (
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
    await tester.pumpAndSettle();
    await _tap(tester, 'review');
    for (var i = 0; i < 20 && !source.controller.hasListener; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(source.controller.hasListener, isTrue);
    expect(_key('cancel-review'), findsOneWidget);
    await _tap(tester, 'cancel-review');
    await tester.pump();
    expect(source.cancelled, isTrue);
    expect(backend.starts, 0);
    expect(backend.executes, 0);
    expect(_enabled(tester, 'review'), isTrue);
  });

  testWidgets('different backend does not inherit prepared execution', (
    tester,
  ) async {
    final first = _Backend();
    await tester.pumpWidget(
      _screen(first, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    await _create(tester);
    await _prepare(tester);
    expect(_enabled(tester, 'execute'), isTrue);
    final second = _Backend();
    await tester.pumpWidget(_screen(second));
    await tester.pump();
    expect(_enabled(tester, 'execute'), isFalse);
    expect(second.starts, 0);
    expect(second.executes, 0);
  });

  testWidgets('catalog revocation disables Execute but preserves cleanup', (
    tester,
  ) async {
    final backend = _Backend();
    await tester.pumpWidget(
      _screen(backend, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    await _create(tester);
    await _prepare(tester);
    expect(_enabled(tester, 'execute'), isTrue);
    await tester.pumpWidget(_screen(backend, authorized: false));
    await tester.pump();
    expect(_enabled(tester, 'execute'), isFalse);
    expect(_enabled(tester, 'release'), isTrue);
    await _tap(tester, 'release');
    expect(backend.releases, 1);
    backend.joined();
    await _tap(tester, 'refresh');
    expect(_enabled(tester, 'ack'), isTrue);
    await _tap(tester, 'ack');
    expect(backend.acks, 1);
    expect(backend.executes, 0);
  });

  testWidgets('operation and cleanup errors retain separate bounded detail', (
    tester,
  ) async {
    final backend = _Backend()..badPrepared = true;
    await tester.pumpWidget(
      _screen(backend, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    await _create(tester);
    await _prepare(tester);
    expect(_key('operation-error'), findsOneWidget);
    expect(_enabled(tester, 'execute'), isFalse);
    backend.failStatus = true;
    await _tap(tester, 'refresh');
    expect(_key('cleanup-error'), findsOneWidget);
    await tester.ensureVisible(_key('operation-error'));
    await tester.tap(_key('operation-error'));
    await tester.pump();
    expect(find.textContaining('operation identity changed'), findsWidgets);
    await tester.ensureVisible(_key('cleanup-error'));
    await tester.tap(_key('cleanup-error'));
    await tester.pump();
    expect(find.textContaining('status diagnostics'), findsWidgets);
  });

  testWidgets('hidden panel does not poll or execute', (tester) async {
    final backend = _Backend();
    await tester.pumpWidget(
      _screen(backend, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    await _create(tester);
    await _prepare(tester);
    await tester.pumpWidget(_screen(backend, visible: false));
    await tester.pump();
    final statuses = backend.statuses;
    await tester.pump(const Duration(seconds: 3));
    expect(backend.statuses, statuses);
    expect(backend.executes, 0);
    await tester.pumpWidget(_screen(backend));
    await tester.pump();
    expect(backend.statuses, greaterThan(statuses));
    expect(backend.executes, 0);
  });

  testWidgets('320px layout retains review and cleanup actions', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(320, 640));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final backend = _Backend();
    await tester.pumpWidget(
      _screen(backend, pickDirectory: () async => r'C:\approved'),
    );
    await tester.pump();
    await _create(tester);
    await _prepare(tester);
    expect(tester.takeException(), isNull);
    expect(_enabled(tester, 'execute'), isTrue);
    expect(_enabled(tester, 'release'), isTrue);
    expect(backend.executes, 0);
    await _tap(tester, 'release');
    expect(backend.releases, 1);
  });
}
