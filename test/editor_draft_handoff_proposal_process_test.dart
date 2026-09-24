import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'fixtures/editor_draft_handoff_proposal_fixture.dart';

final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final root = Platform.environment['MORROW_HANDOFF_PROCESS_ROOT'];
final phase = Platform.environment['MORROW_HANDOFF_PROCESS_PHASE'];
const cases = ['prepare-lost', 'complete-lost', 'retire-lost', 'cancel-lost'];

String hex(List<int> bytes) =>
    bytes.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
Uint8List unhex(String value) => Uint8List.fromList([
  for (var i = 0; i < value.length; i += 2)
    int.parse(value.substring(i, i + 2), radix: 16),
]);
Future<RustWorkbench> open(Directory directory) => RustWorkbench.open(
  executable: python!,
  package: 'proxy',
  directory: directory,
);

Future<void> arm(
  Directory directory,
  wire.Action action,
  EditorDraftHandoffProposal proposal,
) async {
  Future<Uint8List> frame(String transfer) async {
    late Uint8List result;
    await sendHostRequest(
      action,
      configure: (r) {
        r.id = proposal.handoff.request.cardId;
        r.attachment = proposal.handoff.parentLink.parentDraftId;
        r.revision = 0;
        r.operation = proposal.handoff.request.operation;
        r.transfer = transfer;
      },
      send: (bytes) async {
        result = Uint8List.fromList(bytes);
      },
    );
    return result;
  }

  final prefix = action == wire.Action.prepareEditorDraftHandoffProposal
      ? 'upload-'
      : 'draft-request-';
  final a = await frame('${prefix}0');
  final b = await frame('${prefix}9');
  expect(a.length, b.length);
  final mask = [for (var i = 0; i < a.length; i++) a[i] == b[i] ? 255 : 0];
  expect(mask.where((v) => v == 0), hasLength(1));
  await File(
    '${directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': hex(a), 'mask': hex(mask)}));
}

Future<void> seed(
  Directory directory,
  String scenario, {
  bool live = false,
}) async {
  expect(
    await directory.exists(),
    isFalse,
    reason: 'Do not overwrite prior fixtures',
  );
  await directory.create(recursive: true);
  await File(
    'test/fixtures/service_reply_proxy.py',
  ).copy('${directory.path}/workbench.db');
  await File('${directory.path}/proxy.json').writeAsString(
    jsonEncode({
      'host': executable,
      'package': package,
      'store': '${directory.path}/store',
      'mode': live ? 'malformed' : 'eof',
      'trace_requests': true,
    }),
  );
  var host = await open(directory);
  final EditorDraftHandoffProposal proposal;
  try {
    proposal = await seedHandoffProposal(host, directory);
    if (scenario != 'prepare-lost') {
      await host.editorDraftHandoffProposals.prepare(proposal);
    }
    if (scenario == 'retire-lost') {
      await host.editorDraftHandoffProposals.complete(
        cardId: proposal.handoff.request.cardId,
        parentDraftId: proposal.handoff.parentLink.parentDraftId,
        childOperation: proposal.handoff.request.operation,
      );
    }
  } finally {
    await host.close();
  }
  final action = switch (scenario) {
    'prepare-lost' => wire.Action.prepareEditorDraftHandoffProposal,
    'complete-lost' => wire.Action.completeEditorDraftHandoffProposal,
    'retire-lost' => wire.Action.retireEditorDraftHandoffProposal,
    _ => wire.Action.cancelEditorDraftHandoffProposal,
  };
  await arm(directory, action, proposal);
  host = await open(directory);
  try {
    final c = host.editorDraftHandoffProposals;
    final card = proposal.handoff.request.cardId,
        parent = proposal.handoff.parentLink.parentDraftId,
        op = proposal.handoff.request.operation;
    final actionResult = switch (scenario) {
      'prepare-lost' => c.prepare(proposal),
      'complete-lost' => c.complete(
        cardId: card,
        parentDraftId: parent,
        childOperation: op,
      ),
      'retire-lost' => c.retire(
        cardId: card,
        parentDraftId: parent,
        childOperation: op,
      ),
      _ => c.cancel(cardId: card, parentDraftId: parent, childOperation: op),
    };
    await expectLater(
      actionResult,
      throwsA(
        isA<EditorDraftHandoffProposalFailure>().having(
          (e) => e.outcomeUnknown,
          'unknown',
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
      reason: 'real host accepted before lost reply',
    );
    if (live) {
      // The real host remains connected. A new segmented reply must fit after
      // the failed request's known correlation/upload buffer has been aborted.
      final summaries = await c.discover(pageSize: 1);
      expect(summaries, hasLength(1));
      final expected = switch (scenario) {
        'prepare-lost' => EditorDraftHandoffProposalStatus.pending,
        'complete-lost' => EditorDraftHandoffProposalStatus.childCommitted,
        'retire-lost' => EditorDraftHandoffProposalStatus.parentRetired,
        _ => EditorDraftHandoffProposalStatus.cancelled,
      };
      expect(summaries.single.status, expected);
      final restored = await c.inspect(
        cardId: card,
        parentDraftId: parent,
        childOperation: op,
      );
      expect(
        restored!.proposal.handoff.request.values.title.text,
        proposalRawTitle(),
      );
      final events = (await File(
        '${directory.path}/trace.jsonl',
      ).readAsLines()).map(jsonDecode).toList();
      final injectedAt = events.lastIndexWhere((e) => e['event'] == 'injected');
      expect(injectedAt, greaterThanOrEqualTo(0));
      final after = events
          .skip(injectedAt + 1)
          .where((e) => e['event'] == 'request')
          .map(
            (e) => RustWorkbench.readMessage(
              unhex(e['hex'] as String),
            ).getRoot(wire.requestFactory),
          )
          .toList();
      expect(after.first.action, wire.Action.abortEditorDraftTransfer);
      expect(
        after.first.transfer,
        startsWith(scenario == 'prepare-lost' ? 'upload-' : 'draft-request-'),
      );
      expect(
        after.map((r) => r.action),
        isNot(contains(action)),
        reason: 'buffer cleanup must not replay the failed mutation',
      );
    }
    await File('${directory.path}/seed-pid.txt').writeAsString('$pid');
  } finally {
    await host.close();
  }
}

Future<void> recover(Directory directory, String scenario) async {
  expect(
    await File('${directory.path}/seed-pid.txt').readAsString(),
    isNot('$pid'),
  );
  // Recovery receives only the database directory, never a request object or id.
  final trace = File('${directory.path}/trace.jsonl');
  final before = (await trace.readAsLines()).length;
  final host = await open(directory);
  try {
    final c = host.editorDraftHandoffProposals;
    final discovered = await c.discover(pageSize: 1);
    expect(discovered, hasLength(1));
    final s = discovered.single;
    final expected = switch (scenario) {
      'prepare-lost' => EditorDraftHandoffProposalStatus.pending,
      'complete-lost' => EditorDraftHandoffProposalStatus.childCommitted,
      'retire-lost' => EditorDraftHandoffProposalStatus.parentRetired,
      _ => EditorDraftHandoffProposalStatus.cancelled,
    };
    expect(s.status, expected);
    final record = await c.inspect(
      cardId: s.cardId,
      parentDraftId: s.parentDraftId,
      childOperation: s.childOperation,
    );
    expect(
      record!.proposal.handoff.request.values.title.text,
      proposalRawTitle(),
    );
    expect(record.proposal.handoff.request.values.description.composingEnd, 18);
    expect(record.summary.status, expected);
    expect(record.proposal.retirementOperation, s.retirementOperation);
    expect(await File('${directory.path}/source.bin').exists(), isFalse);
    final child = await host.editorDrafts.read(s.cardId, s.childDraftId);
    expect(
      child == null,
      scenario == 'prepare-lost' || scenario == 'cancel-lost',
    );
    final parent = await host.editorDrafts.read(s.cardId, s.parentDraftId);
    expect(parent!.active, scenario != 'retire-lost');
    final actions = (await trace.readAsLines())
        .skip(before)
        .map(jsonDecode)
        .where((e) => e['event'] == 'request')
        .map(
          (e) => RustWorkbench.readMessage(
            unhex(e['hex'] as String),
          ).getRoot(wire.requestFactory).action,
        )
        .toList();
    for (final action in [
      wire.Action.prepareEditorDraftHandoffProposal,
      wire.Action.completeEditorDraftHandoffProposal,
      wire.Action.retireEditorDraftHandoffProposal,
      wire.Action.cancelEditorDraftHandoffProposal,
    ]) {
      expect(
        actions,
        isNot(contains(action)),
        reason: 'discovery must not replay a mutation',
      );
    }
    // Disarm fault injection only after read-only recovery is verified.
    await File('${directory.path}/armed.mask.json').delete();
    if (scenario == 'cancel-lost') {
      await expectLater(
        c.complete(
          cardId: s.cardId,
          parentDraftId: s.parentDraftId,
          childOperation: s.childOperation,
        ),
        throwsA(isA<EditorDraftHandoffProposalFailure>()),
      );
      expect(await host.editorDrafts.read(s.cardId, s.childDraftId), isNull);
    } else {
      if (scenario == 'prepare-lost') {
        await c.complete(
          cardId: s.cardId,
          parentDraftId: s.parentDraftId,
          childOperation: s.childOperation,
        );
      }
      if (scenario != 'retire-lost') {
        await c.retire(
          cardId: s.cardId,
          parentDraftId: s.parentDraftId,
          childOperation: s.childOperation,
        );
      }
      final finalRecord = await c.inspect(
        cardId: s.cardId,
        parentDraftId: s.parentDraftId,
        childOperation: s.childOperation,
      );
      expect(
        finalRecord!.summary.status,
        EditorDraftHandoffProposalStatus.parentRetired,
      );
      final saved = await host.editorDrafts.read(s.cardId, s.childDraftId);
      final path = '${directory.path}/recovered.bin';
      await host.editorDrafts.exportAsset(
        s.cardId,
        s.childDraftId,
        saved!.generation,
        saved.assets.single.selection.assetId,
        path,
      );
      expect(await File(path).readAsBytes(), proposalBytes);
      expect(
        (await host.versionedContent.read(s.cardId)).revision,
        saved.request.sourceRevision,
      );
    }
  } finally {
    await host.close();
  }
}

void main() {
  stdout.writeln('Independent handoff client phase=$phase pid=$pid');
  for (final scenario in cases) {
    test(
      'independent handoff $phase $scenario',
      () async {
        final directory = Directory('$root/$scenario');
        if (phase == 'seed' || phase == 'live') {
          await seed(directory, scenario, live: phase == 'live');
        } else {
          await recover(directory, scenario);
        }
      },
      skip:
          !Platform.isWindows ||
          executable == null ||
          package == null ||
          python == null ||
          root == null ||
          !['seed', 'recover', 'live'].contains(phase),
      timeout: const Timeout(Duration(minutes: 4)),
    );
  }
}
