// Run seed and recover in separate Flutter invocations. The recovery process
// receives only the fixture directory; operation identities come from the host.
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_import_session.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final _root = Platform.environment['MORROW_DECISION_PROCESS_ROOT'];
final _phase = Platform.environment['MORROW_DECISION_PROCESS_PHASE'];
final _available =
    Platform.isWindows &&
    _host != null &&
    _package != null &&
    _python != null &&
    _root != null &&
    (_phase == 'seed' || _phase == 'recover');
const _card = 'process-recovery-card';
const _draft = 'process-recovery-draft';
const _data = [1, 7, 12, 33, 255, 0, 9];
const _cases = [
  'prepared-lost',
  'committed-lost',
  'cancelled-lost',
  'conflict',
  'discarded',
];
EditorDraftTextValue _text() => EditorDraftTextValue(
  text: '',
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftWriteRequest _write(String operation, BigInt generation) =>
    EditorDraftWriteRequest(
      cardId: _card,
      draftId: _draft,
      operation: operation,
      sourceKind: EditorDraftSourceKind.newCard,
      sourceRevision: BigInt.zero,
      expectedGeneration: generation,
      predecessorOperation: '',
      predecessorDigest: [],
      values: EditorDraftValues(
        title: _text(),
        description: _text(),
        hypothesis: _text(),
        conclusion: _text(),
        todos: _text(),
        category: '',
        stage: '',
      ),
      assets: [],
    );
String _fresh(String prefix) =>
    '$prefix-${List.generate(16, (_) => Random.secure().nextInt(256).toRadixString(16).padLeft(2, '0')).join()}';
String _hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
Future<void> _arm(
  Directory directory,
  wire.Action action,
  EditorDraftImportAbandon intent,
) async {
  Future<Uint8List> frame(String correlation) async {
    late Uint8List result;
    await sendHostRequest(
      action,
      configure: (r) {
        r.id = intent.cardId;
        r.attachment = intent.draftId;
        r.revision = 1;
        r.operation = intent.operation;
        r.name = intent.importOperation;
        r.transfer = correlation;
      },
      send: (bytes) async {
        result = Uint8List.fromList(bytes);
      },
    );
    return result;
  }

  final a = await frame('draft-request-0');
  final b = await frame('draft-request-9');
  expect(a.length, b.length);
  final mask = [for (var i = 0; i < a.length; i++) a[i] == b[i] ? 255 : 0];
  expect(mask.where((b) => b == 0), hasLength(1));
  await File(
    '${directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': _hex(a), 'mask': _hex(mask)}));
}

Future<void> _seed(Directory directory, String scenario) async {
  expect(
    await directory.exists(),
    isFalse,
    reason: 'Never overwrite an existing fixture',
  );
  await directory.create(recursive: true);
  await File(
    'test/fixtures/service_reply_proxy.py',
  ).copy('${directory.path}/workbench.db');
  await File('${directory.path}/proxy.json').writeAsString(
    jsonEncode({
      'host': _host,
      'package': _package,
      'store': '${directory.path}/store',
      'mode': 'eof',
      'trace_requests': true,
    }),
  );
  final host = await RustWorkbench.open(
    executable: _python!,
    package: 'proxy',
    directory: directory,
  );
  try {
    await host.editorDrafts.save(_write(_fresh('draft'), BigInt.zero));
    final source = File('${directory.path}/original.bin');
    await source.writeAsBytes(_data);
    final request = EditorDraftImportRequest(
      cardId: _card,
      draftId: _draft,
      operation: _fresh('import'),
      expectedGeneration: BigInt.one,
      name: 'original.bin',
      kind: 'file',
      bytes: BigInt.from(_data.length),
      sha256: sha256.convert(_data).bytes,
    );
    await host.editorDraftImports.complete(request, selectedPath: source.path);
    await source.delete();
    final intent = EditorDraftImportAbandon(
      cardId: _card,
      draftId: _draft,
      importOperation: request.operation,
      operation: _fresh('abandon'),
      currentGeneration: BigInt.one,
    );
    if (scenario == 'conflict' || scenario == 'discarded') {
      await host.editorDraftImports.prepareDecision(intent);
      if (scenario == 'discarded') {
        await host.editorDrafts.discard(
          _card,
          _draft,
          BigInt.one,
          _fresh('discard'),
        );
      } else {
        await host.editorDrafts.save(_write(_fresh('advance'), BigInt.one));
      }
      return;
    }
    final action = switch (scenario) {
      'prepared-lost' => wire.Action.prepareEditorDraftImportDecision,
      'committed-lost' => wire.Action.abandonEditorDraftImport,
      _ => wire.Action.cancelEditorDraftImportDecision,
    };
    if (scenario == 'cancelled-lost') {
      await host.editorDraftImports.prepareDecision(intent);
    }
    await _arm(directory, action, intent);
    await expectLater(
      scenario == 'cancelled-lost'
          ? host.editorDraftImports.cancelDecision(intent)
          : host.editorDraftImports.abandon(intent),
      throwsA(
        isA<EditorDraftImportFailure>().having(
          (e) => e.outcomeUnknown,
          'outcome unknown',
          isTrue,
        ),
      ),
    );
    final receipt = RustWorkbench.readMessage(
      await File('${directory.path}/receipt.bin').readAsBytes(),
    ).getRoot(wire.responseFactory);
    expect(
      receipt.error ?? '',
      isEmpty,
      reason: 'Host really committed before EOF',
    );
    // In the Prepare-loss case, the client must never dispatch action107.
    if (scenario == 'prepared-lost') {
      final events = await File('${directory.path}/trace.jsonl').readAsLines();
      final actions = events
          .map(jsonDecode)
          .where((e) => e['event'] == 'request')
          .map((e) {
            final hex = e['hex'] as String;
            final bytes = [
              for (var i = 0; i < hex.length; i += 2)
                int.parse(hex.substring(i, i + 2), radix: 16),
            ];
            return RustWorkbench.readMessage(
              Uint8List.fromList(bytes),
            ).getRoot(wire.requestFactory).action;
          })
          .toList();
      expect(actions, contains(wire.Action.prepareEditorDraftImportDecision));
      expect(actions, isNot(contains(wire.Action.abandonEditorDraftImport)));
    }
  } finally {
    await host.close();
  }
}

Future<void> _recover(Directory directory, String scenario) async {
  expect(await File('${directory.path}/original.bin').exists(), isFalse);
  // No client intent file, environment operation, or previous session object is
  // consulted. Only the workspace path is supplied by the fixture.
  final host = await RustWorkbench.open(
    executable: _host!,
    package: _package!,
    directory: Directory('${directory.path}/store'),
    managed: true,
  );
  try {
    if (scenario == 'discarded') {
      expect(await host.editorDrafts.list(), isEmpty);
    }
    // Discover even inactive scopes without consulting active drafts or files.
    final scopes = await host.editorDraftImports.listDecisionScopes();
    expect(scopes.scopes, hasLength(1));
    final card = scopes.scopes.single.cardId;
    final draft = scopes.scopes.single.draftId;
    final page = await host.editorDraftImports.listDecisions(card, draft);
    expect(page.decisions, hasLength(1));
    expect(page.nextCursor, isEmpty);
    final decision = page.decisions.single;
    expect(decision.operation, startsWith('abandon-'));
    expect(decision.request.operation, startsWith('import-'));
    final expected = switch (scenario) {
      'prepared-lost' => EditorDraftImportDecisionStatus.pending,
      'committed-lost' => EditorDraftImportDecisionStatus.committed,
      'cancelled-lost' => EditorDraftImportDecisionStatus.cancelled,
      _ => EditorDraftImportDecisionStatus.conflict,
    };
    expect(decision.status, expected);
    final session = EditorDraftImportSession.restoreDecision(
      control: host.editorDraftImports,
      decision: decision,
      isCurrent: () => true,
    );
    expect(session.lastDecision!.operation, decision.operation);
    expect(
      session.confirmed,
      isNull,
      reason: 'Recovery is not a new import execution',
    );
    final before = await host.editorDraftImports.inspect(
      card,
      draft,
      decision.request.operation,
    );
    final again = await host.editorDraftImports.listDecisions(card, draft);
    expect(again.stagingRevision, page.stagingRevision);
    expect(again.decisions.single.decisionRevision, decision.decisionRevision);
    expect(
      before.record!.phase,
      (scenario == 'committed-lost' || scenario == 'discarded')
          ? EditorDraftImportPhase.retired
          : EditorDraftImportPhase.ready,
    );
    if (scenario == 'prepared-lost') {
      expect(session.pendingAbandon!.operation, decision.operation);
      final result = await session.retryAbandon();
      expect(result.record!.phase, EditorDraftImportPhase.retired);
      final inspected = await host.editorDraftImports.inspectDecision(
        decision.intent,
      );
      expect(
        inspected.decision!.status,
        EditorDraftImportDecisionStatus.committed,
      );
      expect(inspected.decision!.operation, decision.operation);
    } else if (scenario == 'conflict' || scenario == 'discarded') {
      expect(session.pendingAbandon!.currentGeneration, BigInt.one);
      expect(decision.currentGeneration, BigInt.two);
      await session.cancelDecision(decision.intent);
      expect(session.pendingAbandon, isNull);
      expect(
        session.lastDecision!.status,
        EditorDraftImportDecisionStatus.cancelled,
      );
      await expectLater(
        host.editorDraftImports.abandon(decision.intent),
        throwsA(isA<EditorDraftImportFailure>()),
      );
      final stillReady = await host.editorDraftImports.inspect(
        card,
        draft,
        decision.request.operation,
      );
      expect(
        stillReady.record!.phase,
        scenario == 'discarded'
            ? EditorDraftImportPhase.retired
            : EditorDraftImportPhase.ready,
      );
    } else {
      expect(session.pendingAbandon, isNull);
      expect(() => session.retryAbandon(), throwsStateError);
      if (scenario == 'committed-lost') {
        final repeated = await host.editorDraftImports.abandon(decision.intent);
        expect(repeated.record!.repeated, isTrue);
        expect(repeated.stagingRevision, page.stagingRevision);
        await host.editorDraftImports.reconcile(card, draft);
        final afterCleanup = await host.editorDraftImports.inspectDecision(
          decision.intent,
        );
        expect(
          afterCleanup.decision!.status,
          EditorDraftImportDecisionStatus.committed,
        );
        expect(
          afterCleanup.decision!.committedRevision,
          decision.committedRevision,
        );
      } else {
        await expectLater(
          host.editorDraftImports.abandon(decision.intent),
          throwsA(isA<EditorDraftImportFailure>()),
        );
        final repeated = await host.editorDraftImports.cancelDecision(
          decision.intent,
        );
        expect(repeated.decision!.decisionRevision, BigInt.two);
      }
    }
    session.dispose();
  } finally {
    await host.close();
  }
}

void main() {
  stdout.writeln('Independent recovery client phase=$_phase pid=$pid');
  for (final scenario in _cases) {
    test(
      'independent client process $_phase $scenario',
      () async {
        final directory = Directory('$_root/$scenario');
        if (_phase == 'seed') {
          await _seed(directory, scenario);
        } else {
          await _recover(directory, scenario);
        }
      },
      skip: !_available,
      timeout: const Timeout(Duration(minutes: 4)),
    );
  }
}
