import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];

final class _OriginalImport implements EditorDraftImportControl {
  _OriginalImport(this.native, this.loseComplete);
  final EditorDraftImportControl native;
  final bool loseComplete;
  int begins = 0, completes = 0, inspections = 0;
  @override
  Future<EditorDraftImportSnapshot> begin(EditorDraftImportRequest request) {
    begins++;
    return native.begin(request);
  }

  @override
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest request, {
    String selectedPath = '',
  }) async {
    completes++;
    final reply = await native.complete(request, selectedPath: selectedPath);
    if (loseComplete) throw StateError('withheld actual Ready receipt');
    return reply;
  }

  @override
  Future<EditorDraftImportSnapshot> inspect(
    String card,
    String draft,
    String operation,
  ) {
    inspections++;
    return native.inspect(card, draft, operation);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

EditorDraftTextValue _raw(String text) => EditorDraftTextValue(
  text: text,
  selectionBase: 0,
  selectionExtent: text.length,
  affinity: 0,
  directional: true,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftValues _values(String body) => EditorDraftValues(
  title: _raw(''),
  description: _raw(body),
  hypothesis: _raw('假设'),
  conclusion: _raw('结论'),
  todos: _raw('one\n\ntwo'),
  category: '实验',
  stage: '待验证',
);

void main() {
  for (final fault in ['complete', 'export']) {
    test(
      'actual original import $fault loss resolves by inspection after source deletion',
      () async {
        final root = await Directory.systemTemp.createTemp(
          'morrow-import-owner-',
        );
        final store = Directory('${root.path}/store');
        var host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: store,
        );
        var closed = false;
        try {
          final editor = await host.openEditor(
            'import-owned-card',
            create: true,
          );
          final owner = await host.createEditorDraftOwner(
            editor: editor,
            initial: EditorDraftSnapshot(
              values: _values('before'),
              assets: const [],
            ),
            attachments: const [],
            create: true,
          );
          owner.session.pauseAutoSave();
          const bytes = [0, 255, 4, 9, 28, 73];
          final selected = File('${root.path}/selected.bin');
          await selected.writeAsBytes(bytes);
          final control = _OriginalImport(
            host.editorDraftImports,
            fault == 'complete',
          );
          var exports = 0;
          await expectLater(
            owner.importAsset(
              control: control,
              selectedPath: selected.path,
              prepare: (record) async => EditorDraftImportRequest(
                cardId: owner.session.cardId,
                draftId: owner.session.draftId,
                operation: 'original-import-operation',
                expectedGeneration: record.generation,
                name: 'selected.bin',
                kind: 'file',
                bytes: BigInt.from(bytes.length),
                sha256: sha256.convert(bytes).bytes,
              ),
              export: (ready) async {
                if (exports++ == 0 && fault == 'export') {
                  throw StateError('preview export unavailable');
                }
                final file = File('${root.path}/verified-preview.bin');
                final reply = await host.editorDraftImports.export(
                  owner.session.cardId,
                  owner.session.draftId,
                  ready.currentGeneration,
                  ready.request.operation,
                  file.path,
                );
                final actual = await file.readAsBytes();
                expect(actual, bytes);
                expect(reply.exportBytes, BigInt.from(bytes.length));
                expect(reply.exportSha256, sha256.convert(bytes).bytes);
                return EditorDraftAssetView(
                  attachment: IdeaAttachment.versioned(
                    source: TextureSource(
                      location: file.path,
                      name: 'selected.bin',
                      kind: TextureKind.file,
                      local: true,
                    ),
                    byteLength: ready.request.bytes,
                    pluginId: ready.assetId,
                  ),
                  aliases: [file.path, 'selected.bin'],
                  sha256: sha256.convert(actual).bytes,
                );
              },
            ),
            throwsA(isA<Object>()),
          );
          expect(owner.hasUnresolvedImports, isTrue);
          await selected.delete();
          owner.session.observeIncompleteValues(
            _values('S2😀 preserved during failure'),
            StateError('original import unresolved'),
          );
          final view = await owner.inspectOriginalImport();
          expect(control.begins, 1);
          expect(control.completes, 1);
          expect(control.inspections, 1);
          expect(owner.hasUnresolvedImports, isFalse);
          expect(
            owner.session.current.values.description.text,
            'S2😀 preserved during failure',
          );
          final record = (await owner.session.flushLatestVisible(
            timeout: const Duration(seconds: 20),
          ))!;
          expect(
            record.assets.single.selection.assetId,
            view.attachment.pluginId,
          );
          final prepared = await host.prepareEditorDraftClose();
          await host.finishEditorDraftClose(prepared);
          await host.close();
          closed = true;
          host = await RustWorkbench.open(
            executable: _host!,
            package: _package!,
            directory: store,
          );
          closed = false;
          final restored = (await host.editorDrafts.read(
            record.request.cardId,
            record.request.draftId,
          ))!;
          expect(
            restored.request.values.description.text,
            'S2😀 preserved during failure',
          );
          expect(restored.request.values.description.directional, isTrue);
          final exported = File('${root.path}/after-restart.bin');
          await host.editorDrafts.exportAsset(
            record.request.cardId,
            record.request.draftId,
            restored.generation,
            restored.assets.single.selection.assetId,
            exported.path,
          );
          expect(await exported.readAsBytes(), bytes);
          expect(await selected.exists(), isFalse);
          await host.close();
          closed = true;
        } finally {
          if (closed) await root.delete(recursive: true);
        }
      },
      skip: !Platform.isWindows || _host == null || _package == null,
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
}
