import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_import_codec.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _hostPath = Platform.environment['MORROW_WORKBENCH_HOST'];
final _packagePath = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
final _available =
    Platform.isWindows && _hostPath != null && _packagePath != null;
const _card = 'durable-import-native-new-card';
const _draft = 'durable-import-native-draft';
final _data = List<int>.generate(48000, (i) => i % 241);

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: value.length,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftWriteRequest _draftRequest({
  String operation = 'native-draft-create',
  BigInt? generation,
  List<EditorDraftAssetSelection> assets = const [],
}) => EditorDraftWriteRequest(
  cardId: _card,
  draftId: _draft,
  operation: operation,
  sourceKind: EditorDraftSourceKind.newCard,
  sourceRevision: BigInt.zero,
  expectedGeneration: generation ?? BigInt.zero,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: EditorDraftValues(
    title: _text(''),
    description: _text('draft 😀 ' * 12000),
    hypothesis: _text(''),
    conclusion: _text(''),
    todos: _text(''),
    category: '',
    stage: '',
  ),
  assets: assets,
);
EditorDraftImportRequest _request(
  String operation, {
  String name = 'original.bin',
}) => EditorDraftImportRequest(
  cardId: _card,
  draftId: _draft,
  operation: operation,
  expectedGeneration: BigInt.one,
  name: name,
  kind: 'file',
  bytes: BigInt.from(_data.length),
  sha256: sha256.convert(_data).bytes,
);
EditorDraftImportAbandon _abandon(EditorDraftImportRequest request) =>
    EditorDraftImportAbandon(
      cardId: _card,
      draftId: _draft,
      importOperation: request.operation,
      operation: 'explicit-abandon-import',
      currentGeneration: BigInt.one,
    );
Future<RustWorkbench> _open(Directory directory) => RustWorkbench.open(
  executable: _hostPath!,
  package: _packagePath!,
  directory: directory,
);
String _hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
Future<void> _arm(
  Directory directory,
  wire.Action action,
  EditorDraftImportRequest intent,
  String source,
) async {
  Future<Uint8List> frame(String correlation) async {
    late Uint8List result;
    await sendHostRequest(
      action,
      configure: (outer) {
        outer.id = _card;
        outer.attachment = _draft;
        outer.revision = 1;
        outer.operation = action == wire.Action.completeEditorDraftImport
            ? intent.operation
            : _abandon(intent).operation;
        outer.name = intent.operation;
        if (action == wire.Action.completeEditorDraftImport) {
          outer.payload = EditorDraftImportCodec.encodeRequest(intent);
          outer.selectedPath = source;
        }
        outer.transfer = correlation;
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
  expect(mask.where((value) => value == 0), hasLength(1));
  await File(
    '${directory.path}/armed.mask.json',
  ).writeAsString(jsonEncode({'template': _hex(a), 'mask': _hex(mask)}));
}

void main() {
  test(
    'durable import shares full transfer queue with drafts and confirmed pins survive cleanup',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-durable-native-',
      );
      RustWorkbench? host;
      try {
        host = await _open(directory);
        await host.editorDrafts.save(_draftRequest());
        final source = File('${directory.path}/original.bin');
        await source.writeAsBytes(_data);
        final intents = List.generate(
          3,
          (i) => _request('pending-$i', name: '${'n' * 15000}-$i'),
        );
        for (final intent in intents) {
          final pending = await host.editorDraftImports.begin(intent);
          expect(pending.record!.phase, EditorDraftImportPhase.pending);
        }
        // Both readers need multiple chunks. RPC-only serialization would collide.
        final results = await Future.wait<Object?>([
          host.editorDrafts.read(_card, _draft),
          host.editorDraftImports.list(_card, _draft),
          host.editorDrafts.read(_card, _draft),
          host.editorDraftImports.list(_card, _draft),
        ]);
        expect((results[1] as EditorDraftImportSnapshot).records, hasLength(3));
        expect((results[2] as EditorDraftRecord).generation, BigInt.one);
        final ready = await host.editorDraftImports.complete(
          intents[0],
          selectedPath: source.path,
        );
        expect(ready.record!.currentActive, isTrue);
        await source.delete();
        final repeated = await host.editorDraftImports.complete(
          intents[0],
          selectedPath: source.path,
        );
        expect(repeated.record!.repeated, isTrue);
        final target = '${directory.path}/exported.bin';
        await host.editorDraftImports.export(
          _card,
          _draft,
          BigInt.one,
          intents[0].operation,
          target,
        );
        await host.editorDraftImports.export(
          _card,
          _draft,
          BigInt.one,
          intents[0].operation,
          target,
        );
        expect(await File(target).readAsBytes(), _data);
        final selected = await host.editorDrafts.save(
          _draftRequest(
            operation: 'select-native-import',
            generation: BigInt.one,
            assets: [
              EditorDraftAssetSelection(
                origin: EditorDraftAssetOrigin.staged,
                assetId: ready.record!.assetId,
                aliases: const [],
              ),
            ],
          ),
        );
        expect(selected.generation, BigInt.two);
        await host.editorDraftImports.reconcile(_card, _draft);
        await host.close();
        host = await _open(directory);
        final retired = await host.editorDraftImports.inspect(
          _card,
          _draft,
          intents[0].operation,
        );
        expect(retired.record!.phase, EditorDraftImportPhase.retired);
        expect(retired.record!.bytesRetained, isFalse);
        final selectedExport = '${directory.path}/selected-after-restart.bin';
        await host.editorDrafts.exportAsset(
          _card,
          _draft,
          BigInt.two,
          ready.record!.assetId,
          selectedExport,
        );
        expect(await File(selectedExport).readAsBytes(), _data);
      } finally {
        await host?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );

  for (final mode in ['replace', 'eof']) {
    for (final action in [
      wire.Action.completeEditorDraftImport,
      wire.Action.abandonEditorDraftImport,
    ]) {
      test(
        'lost ${action.name} reply $mode retains original intent and reconciles across restart',
        () async {
          final directory = await Directory.systemTemp.createTemp(
            'morrow-import-reply-',
          );
          RustWorkbench? host;
          try {
            await File(
              'test/fixtures/service_reply_proxy.py',
            ).copy('${directory.path}/workbench.db');
            await File('${directory.path}/proxy.json').writeAsString(
              jsonEncode({
                'host': _hostPath,
                'package': _packagePath,
                'store': '${directory.path}/store',
                'mode': mode,
                'trace_requests': true,
              }),
            );
            final replacement = MessageBuilder();
            final error = replacement.initRoot(wire.responseFactory);
            error.version = 1;
            error.digest = Uint8List.fromList(contract.hostDigest);
            error.error = 'injected durable import response loss';
            await File(
              '${directory.path}/replacement.bin',
            ).writeAsBytes(replacement.serialize());
            host = await RustWorkbench.open(
              executable: _python!,
              package: 'proxy',
              directory: directory,
            );
            await host.editorDrafts.save(_draftRequest());
            final source = File('${directory.path}/original.bin');
            await source.writeAsBytes(_data);
            final intent = _request('lost-import');
            final abandon = _abandon(intent);
            if (action == wire.Action.abandonEditorDraftImport) {
              await host.editorDraftImports.complete(
                intent,
                selectedPath: source.path,
              );
            }
            await _arm(directory, action, intent, source.path);
            final Object frozen =
                action == wire.Action.completeEditorDraftImport
                ? intent
                : abandon;
            await expectLater(
              action == wire.Action.completeEditorDraftImport
                  ? host.editorDraftImports.complete(
                      intent,
                      selectedPath: source.path,
                    )
                  : host.editorDraftImports.abandon(abandon),
              throwsA(
                isA<EditorDraftImportFailure>()
                    .having((e) => e.outcomeUnknown, 'unknown effect', isTrue)
                    .having(
                      (e) => identical(e.intent, frozen),
                      'frozen exact intent',
                      isTrue,
                    ),
              ),
            );
            final actual = RustWorkbench.readMessage(
              await File('${directory.path}/receipt.bin').readAsBytes(),
            ).getRoot(wire.responseFactory);
            expect(actual.error ?? '', isEmpty);
            expect(actual.revision, 1);
            await source.delete();
            if (mode == 'replace') {
              // Aborting a lost response frees only its buffer; not the journal.
              final inspected = await host.editorDraftImports.inspect(
                _card,
                _draft,
                intent.operation,
              );
              expect(
                inspected.record!.phase,
                action == wire.Action.completeEditorDraftImport
                    ? EditorDraftImportPhase.ready
                    : EditorDraftImportPhase.retired,
              );
            }
            await host.close();
            host = await RustWorkbench.open(
              executable: _hostPath!,
              package: _packagePath!,
              directory: Directory('${directory.path}/store'),
              managed: true,
            );
            final restored = await host.editorDraftImports.inspect(
              _card,
              _draft,
              intent.operation,
            );
            expect(restored.record!.request.operation, intent.operation);
            if (action == wire.Action.completeEditorDraftImport) {
              final confirmed = await host.editorDraftImports.complete(
                intent,
                selectedPath: source.path,
              );
              expect(confirmed.record!.repeated, isTrue);
              expect(confirmed.record!.currentActive, isTrue);
              final target = '${directory.path}/recovered.bin';
              await host.editorDraftImports.export(
                _card,
                _draft,
                BigInt.one,
                intent.operation,
                target,
              );
              expect(await File(target).readAsBytes(), _data);
            } else {
              expect(restored.record!.phase, EditorDraftImportPhase.retired);
              final confirmed = await host.editorDraftImports.abandon(abandon);
              expect(confirmed.record!.repeated, isTrue);
              final cleaned = await host.editorDraftImports.reconcile(
                _card,
                _draft,
              );
              expect(cleaned.records, isEmpty);
              final old = await host.editorDraftImports.complete(
                intent,
                selectedPath: source.path,
              );
              expect(old.record!.currentActive, isFalse);
            }
          } finally {
            await host?.close();
            await directory.delete(recursive: true);
          }
        },
        skip: !_available || _python == null,
        timeout: const Timeout(Duration(minutes: 4)),
      );
    }
  }
}
