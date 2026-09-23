import 'dart:async';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

final _host = Platform.environment['MORROW_WORKBENCH_HOST'];
final _package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
final _available = Platform.isWindows && _host != null && _package != null;

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: 0,
  selectionExtent: value.length,
  affinity: 1,
  directional: true,
  composingStart: -1,
  composingEnd: -1,
);
EditorDraftSnapshot _snapshot(
  String title,
  List<EditorDraftAssetSelection> assets,
) => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: _text(title),
    description: _text('正文😀'),
    hypothesis: _text('假设'),
    conclusion: _text('结论'),
    todos: _text('- [ ] 未完成'),
    category: '研究',
    stage: '草稿',
  ),
  assets: assets,
);

// Hold only the acknowledgement, after the real host has durably saved S1.
final class _HeldReceiptControl implements EditorDraftControl {
  _HeldReceiptControl(this.delegate);
  final EditorDraftControl delegate;
  bool holdNext = false;
  final committed = Completer<EditorDraftRecord>();
  final release = Completer<void>();
  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) async {
    final hold = holdNext;
    holdNext = false;
    final record = await delegate.save(request);
    if (hold) {
      committed.complete(record);
      await release.future;
    }
    return record;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test(
    'detached S2 keeps two durable imports through S1, close, and host restart',
    () async {
      final root = await Directory.systemTemp.createTemp('morrow-live-draft-');
      RustWorkbench? host;
      EditorDraftSession? active;
      final bytes = <List<int>>[
        [1, 2, 3, 9],
        [8, 7, 6, 4, 2],
      ];
      try {
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final control = _HeldReceiptControl(host.editorDrafts);
        var operation = 0;
        active = EditorDraftSession.newSession(
          control: control,
          cardId: 'live-card',
          draftId: 'live-draft',
          sourceRevision: BigInt.zero,
          sourceKind: EditorDraftSourceKind.newCard,
          initialSnapshot: _snapshot('', const []),
          operationFactory: () => 'live-save-${++operation}',
          isCurrent: () => true,
          debounce: null,
        );
        final session = active;
        final owner = EditorDraftWorkspace(isCurrent: () => true);
        final firstLease = owner.attach(session);
        final first = await session.ensureJournal();
        final ready = <EditorDraftImportRecord>[];
        final views = <EditorDraftAssetView>[];
        for (var i = 0; i < bytes.length; i++) {
          final file = File('${root.path}/picked-$i.bin');
          await file.writeAsBytes(bytes[i]);
          final digest = sha256.convert(bytes[i]).bytes;
          final result = await host.editorDraftImports.complete(
            EditorDraftImportRequest(
              cardId: 'live-card',
              draftId: 'live-draft',
              operation: 'live-import-$i',
              expectedGeneration: BigInt.one,
              name: 'picked-$i.bin',
              kind: 'file',
              bytes: BigInt.from(bytes[i].length),
              sha256: digest,
            ),
            selectedPath: file.path,
          );
          ready.add(result.record!);
          views.add(
            EditorDraftAssetView(
              attachment: IdeaAttachment.versioned(
                source: TextureSource(
                  location: file.path,
                  name: 'picked-$i.bin',
                  kind: TextureKind.file,
                  local: true,
                ),
                byteLength: BigInt.from(bytes[i].length),
                pluginId: result.record!.assetId,
              ),
              aliases: ['picked-$i.bin', 'alias-$i'],
              sha256: digest,
            ),
          );
          await file.delete();
        }
        var catalog = EditorDraftAssetCatalog.newCard(
          cardId: 'live-card',
          draftId: 'live-draft',
          previousDraft: first,
          readyImports: ready,
          readyViews: views,
        );
        session.observe(
          _snapshot('S1', catalog.selectionsFor([views[0].attachment])),
        );
        control.holdNext = true;
        final s1 = session.flush();
        await control.committed.future;
        final allViews = views.map((view) => view.attachment).toList();
        session.observe(_snapshot('S2😀', catalog.selectionsFor(allViews)));
        final generation = session.localGeneration;
        firstLease.release();
        expect(owner.find('live-card', 'live-draft'), same(session));
        expect(session.saving, isTrue);
        control.release.complete();
        final confirmed = (await s1)!;
        catalog = catalog.afterConfirmedDraft(confirmed, [views[0]]);
        session.adoptConfirmedAssetPins(confirmed);
        session.observe(_snapshot('S2😀', catalog.selectionsFor(allViews)));
        expect(session.localGeneration, generation);
        expect(session.current.values.title.text, 'S2😀');
        expect(session.current.assets.map((asset) => asset.origin), [
          EditorDraftAssetOrigin.previousDraft,
          EditorDraftAssetOrigin.staged,
        ]);
        final secondLease = owner.attach(session);
        final preparation = await owner.prepareClose(
          timeout: const Duration(seconds: 20),
        );
        expect(preparation.records.single.generation, BigInt.from(3));
        secondLease.release();
        owner.finishClose(preparation);
        expect(session.disposed, isTrue);
        await host.close();
        host = await RustWorkbench.open(
          executable: _host!,
          package: _package!,
          directory: Directory('${root.path}/store'),
        );
        final restored = (await host.editorDrafts.read(
          'live-card',
          'live-draft',
        ))!;
        expect(restored.request.values.title.text, 'S2😀');
        expect(restored.request.values.title.selectionExtent, 'S2😀'.length);
        expect(restored.request.values.title.directional, isTrue);
        expect(restored.request.values.description.text, '正文😀');
        expect(restored.assets, hasLength(2));
        for (var i = 0; i < bytes.length; i++) {
          final exported = File('${root.path}/exported-$i.bin');
          await host.editorDrafts.exportAsset(
            'live-card',
            'live-draft',
            restored.generation,
            restored.assets[i].selection.assetId,
            exported.path,
          );
          expect(await exported.readAsBytes(), bytes[i]);
          expect(restored.assets[i].selection.aliases, [
            'picked-$i.bin',
            'alias-$i',
          ]);
        }
        active = EditorDraftSession.restore(
          control: host.editorDrafts,
          record: restored,
          operationFactory: () => 'live-save-${++operation}',
          isCurrent: () => true,
          debounce: null,
        );
        final resumed = active;
        final restoredCatalog = EditorDraftAssetCatalog.newCard(
          cardId: 'live-card',
          draftId: 'live-draft',
          previousDraft: restored,
          previousViews: views,
        );
        resumed.adoptConfirmedAssetPins(restored);
        resumed.observe(
          _snapshot('S2😀', restoredCatalog.selectionsFor(allViews)),
        );
        expect(resumed.dirty, isFalse);
        resumed.observe(
          _snapshot('S3', restoredCatalog.selectionsFor(allViews)),
        );
        final next = (await resumed.flush())!;
        expect(next.generation, BigInt.from(4));
        expect(next.assets, hasLength(2));
      } finally {
        active?.dispose();
        await host?.close();
        await root.delete(recursive: true);
      }
    },
    skip: !_available,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
