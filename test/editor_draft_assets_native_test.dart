import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available = Platform.isWindows && _host != null && _package != null;
final _bytes = [42, 17, 9, 88];
final _digest = sha256.convert(_bytes).bytes;

EditorDraftTextValue _text() => EditorDraftTextValue(
  text: '',
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftWriteRequest _write(
  String operation,
  BigInt generation,
  List<EditorDraftAssetSelection> assets,
) => EditorDraftWriteRequest(
  cardId: 'asset-map-card',
  draftId: 'asset-map-draft',
  operation: operation,
  expectedGeneration: generation,
  sourceRevision: BigInt.zero,
  sourceKind: EditorDraftSourceKind.newCard,
  predecessorOperation: '',
  predecessorDigest: const [],
  values: EditorDraftValues(
    title: _text(),
    description: _text(),
    hypothesis: _text(),
    conclusion: _text(),
    todos: _text(),
    category: '',
    stage: '',
  ),
  assets: assets,
);

void main() {
  test(
    'host Ready import maps to staged, then confirmed pin maps to previousDraft',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-draft-assets-',
      );
      RustWorkbench? workbench;
      try {
        workbench = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${directory.path}/store'),
        );
        final first = await workbench.editorDrafts.save(
          _write('asset-first', BigInt.zero, const []),
        );
        final file = File('${directory.path}/picked.bin');
        await file.writeAsBytes(_bytes);
        final request = EditorDraftImportRequest(
          cardId: 'asset-map-card',
          draftId: 'asset-map-draft',
          operation: 'asset-import',
          expectedGeneration: BigInt.one,
          name: 'picked.bin',
          kind: 'file',
          bytes: BigInt.from(_bytes.length),
          sha256: _digest,
        );
        final ready = await workbench.editorDraftImports.complete(
          request,
          selectedPath: file.path,
        );
        expect(ready.record!.phase, EditorDraftImportPhase.ready);
        final attachment = IdeaAttachment.versioned(
          source: TextureSource(
            location: file.path,
            name: 'picked.bin',
            kind: TextureKind.file,
            local: true,
          ),
          byteLength: BigInt.from(_bytes.length),
          pluginId: ready.record!.assetId,
        );
        final view = EditorDraftAssetView(
          attachment: attachment,
          aliases: const ['picked.bin'],
          sha256: _digest,
        );
        final catalog = EditorDraftAssetCatalog.newCard(
          cardId: 'asset-map-card',
          draftId: 'asset-map-draft',
          previousDraft: first,
          readyImports: [ready.record!],
          readyViews: [view],
        );
        final staged = catalog.selectionsFor([attachment]);
        expect(staged.single.origin, EditorDraftAssetOrigin.staged);
        final confirmed = await workbench.editorDrafts.save(
          _write('asset-second', BigInt.one, staged),
        );
        final promoted = catalog.afterConfirmedDraft(confirmed, [view]);
        expect(
          promoted.selectionsFor([attachment]).single.origin,
          EditorDraftAssetOrigin.previousDraft,
        );
        await workbench.close();
        workbench = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${directory.path}/store'),
        );
        final restored = await workbench.editorDrafts.read(
          'asset-map-card',
          'asset-map-draft',
        );
        expect(restored, isNotNull);
        final exact = EditorDraftAssetCatalog.restore(restored!, [view]);
        expect(
          exact.selectionsFor([attachment]).single.origin,
          EditorDraftAssetOrigin.staged,
        );
        final resumed = EditorDraftAssetCatalog.newCard(
          cardId: 'asset-map-card',
          draftId: 'asset-map-draft',
          previousDraft: restored,
          previousViews: [view],
        );
        expect(
          resumed.selectionsFor([attachment]).single.origin,
          EditorDraftAssetOrigin.previousDraft,
        );
      } finally {
        await workbench?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
