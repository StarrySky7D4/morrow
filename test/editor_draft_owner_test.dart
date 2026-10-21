import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_assets.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_owner.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';
import 'package:morrow_studio/plugins/editor_draft_workspace.dart';
import 'package:morrow_studio/plugins/editor_session.dart';

final class _Editor
    implements
        WorkbenchEditorSession,
        WorkbenchEditorOperationIdentity,
        WorkbenchEditorDraftCaptureIdentity {
  Completer<void>? paste;
  int pasteCalls = 0, events = 0;
  @override
  ({String scope, int events, bool complete}) get draftCapture =>
      (scope: 'original-unit-scope', events: events, complete: true);
  @override
  Future<void> recordPaste(PasteInsertion insertion) async {
    pasteCalls++;
    await paste?.future;
    events++;
  }

  @override
  String get commitOperation => 'original-business-operation';
  @override
  BigInt get sourceRevision => BigInt.zero;
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

final class _Drafts implements EditorDraftControl {
  Completer<void>? hold;
  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) async {
    await hold?.future;
    return EditorDraftRecord(
      request: request,
      generation: request.expectedGeneration + BigInt.one,
      active: true,
      currentGeneration: request.expectedGeneration + BigInt.one,
      currentActive: true,
      repeated: false,
      sourceFormat: 0,
      sourceRevision: BigInt.zero,
      sourceSha256: const [],
      predecessorRevision: BigInt.zero,
      predecessorSha256: const [],
      assets: const [],
    );
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

final class _Imports implements EditorDraftImportControl {
  int writes = 0;
  EditorDraftImportRequest? original;
  EditorDraftImportSnapshot _reply(
    EditorDraftImportRequest request,
    bool ready, {
    bool inspected = false,
  }) {
    if (!inspected) writes++;
    original = request;
    return EditorDraftImportSnapshot(
      kind: EditorDraftImportResultKind.record,
      cardId: request.cardId,
      draftId: request.draftId,
      operation: request.operation,
      expectedGeneration: inspected ? BigInt.zero : request.expectedGeneration,
      importOperation: request.operation,
      currentGeneration: request.expectedGeneration,
      mainActive: true,
      stagingRevision: BigInt.two,
      exportBytes: BigInt.zero,
      record: EditorDraftImportRecord(
        request: request,
        assetId: 'asset-one',
        phase: ready
            ? EditorDraftImportPhase.ready
            : EditorDraftImportPhase.pending,
        currentActive: ready,
        bytesRetained: ready,
        stagingRevision: BigInt.two,
        repeated: false,
        currentGeneration: request.expectedGeneration,
        mainActive: true,
      ),
    );
  }

  @override
  Future<EditorDraftImportSnapshot> begin(
    EditorDraftImportRequest request,
  ) async => _reply(request, false);
  @override
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest request, {
    String selectedPath = '',
  }) async => _reply(request, true);
  @override
  Future<EditorDraftImportSnapshot> inspect(
    String card,
    String draft,
    String operation,
  ) async => _reply(original!, true, inspected: true);
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

EditorDraftTextValue _text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: -1,
  selectionExtent: -1,
  affinity: 0,
  directional: false,
  composingStart: -1,
  composingEnd: -1,
);

EditorDraftOwner _owner(_Drafts control, {_Editor? editor}) {
  var sequence = 0;
  final owner = EditorDraftOwner(
    workspace: EditorDraftWorkspace(isCurrent: () => true),
    control: control,
    editor: editor ?? _Editor(),
    catalog: EditorDraftAssetCatalog.newCard(
      cardId: 'card-one',
      draftId: 'draft-one',
    ),
    initial: EditorDraftSnapshot(
      values: EditorDraftValues(
        title: _text(''),
        description: _text('raw body'),
        hypothesis: _text(''),
        conclusion: _text(''),
        todos: _text(''),
        category: 'category',
        stage: 'stage',
      ),
      assets: const [],
    ),
    isCurrent: () => true,
    operationFactory: () => 'draft-operation-${sequence++}',
  );
  owner.session.pauseAutoSave();
  addTearDown(owner.session.dispose);
  return owner;
}

Future<EditorDraftImportRequest> _prepare(EditorDraftRecord record) async =>
    EditorDraftImportRequest(
      cardId: record.request.cardId,
      draftId: record.request.draftId,
      operation: 'original-import',
      expectedGeneration: record.generation,
      name: 'selected.bin',
      kind: 'file',
      bytes: BigInt.from(3),
      sha256: List.filled(32, 4),
    );

Future<EditorDraftAssetView> _export(EditorDraftImportRecord record) async =>
    EditorDraftAssetView(
      attachment: IdeaAttachment.versioned(
        source: TextureSource(
          location: 'private-preview.bin',
          name: record.request.name,
          kind: TextureKind.file,
          local: true,
        ),
        byteLength: record.request.bytes,
        pluginId: record.assetId,
      ),
      aliases: const ['private-preview.bin'],
      sha256: record.request.sha256,
    );

void main() {
  test(
    'explicit Ready inspection repairs export failure without old mutations',
    () async {
      final owner = _owner(_Drafts());
      final imports = _Imports();
      var exports = 0;
      await expectLater(
        owner.importAsset(
          control: imports,
          prepare: _prepare,
          selectedPath: 'original.bin',
          export: (ready) async {
            if (exports++ == 0) throw StateError('export unavailable');
            return _export(ready);
          },
        ),
        throwsStateError,
      );
      owner.session.observeIncompleteValues(
        EditorDraftValues(
          title: _text(''),
          description: _text('new raw while export failed'),
          hypothesis: _text('h'),
          conclusion: _text('c'),
          todos: _text('one\n\ntwo'),
          category: '实验',
          stage: '待验证',
        ),
        StateError('unresolved original import'),
      );
      final view = await owner.inspectOriginalImport();
      expect(view.attachment.pluginId, 'asset-one');
      expect(imports.writes, 2);
      expect(exports, 2);
      expect(owner.hasUnresolvedImports, isFalse);
      expect(owner.session.captureBlocked, isFalse);
      expect(
        owner.session.current.values.description.text,
        'new raw while export failed',
      );
      expect(owner.session.current.assets.single.assetId, 'asset-one');
      await expectLater(owner.inspectOriginalImport(), throwsStateError);
      expect(imports.writes, 2);
    },
  );
  const insertion = PasteInsertion(
    id: 'original-paste',
    field: 'description',
    before: 'raw body',
    startUtf16: 0,
    endUtf16: 8,
    parts: [PastePart.literal('new 😀')],
    after: 'new 😀',
  );

  test('original paste belongs to the root when no view remains', () async {
    final editor = _Editor()..paste = Completer<void>();
    final owner = _owner(_Drafts(), editor: editor);
    final request = owner.recordPaste(insertion);
    expect(owner.captureIdentity.complete, isFalse);
    await expectLater(owner.recordPaste(insertion), throwsStateError);
    editor.paste!.complete();
    final applied = await request;
    expect(applied.selectionExtent, 'new 😀'.length);
    expect(owner.session.current.values.description.text, 'new 😀');
    expect(owner.session.current.captureEvents, 1);
    expect(owner.session.captureBlocked, isFalse);
    expect(editor.pasteCalls, 1);
  });

  test(
    'input during paste is retained and the original intent is not replayed',
    () async {
      final editor = _Editor()..paste = Completer<void>();
      final owner = _owner(_Drafts(), editor: editor);
      final request = owner.recordPaste(insertion);
      final values = owner.session.current.values;
      owner.session.observeIncompleteValues(
        EditorDraftValues(
          title: values.title,
          description: _text('typed while awaiting paste'),
          hypothesis: values.hypothesis,
          conclusion: values.conclusion,
          todos: values.todos,
          category: 'changed',
          stage: 'stage',
        ),
        StateError('paste pending'),
      );
      editor.paste!.complete();
      await expectLater(request, throwsStateError);
      expect(
        owner.session.current.values.description.text,
        'typed while awaiting paste',
      );
      expect(owner.session.captureBlocked, isTrue);
      expect(owner.captureIdentity.complete, isFalse);
      await expectLater(owner.recordPaste(insertion), throwsStateError);
      expect(editor.pasteCalls, 1);
    },
  );

  test('import admission precedes the initial journal await', () async {
    final control = _Drafts()..hold = Completer<void>();
    final owner = _owner(control);
    final imports = _Imports();
    final first = owner.importAsset(
      control: imports,
      prepare: _prepare,
      selectedPath: 'selected.bin',
      export: _export,
    );
    expect(owner.importing, isTrue);
    await expectLater(
      owner.importAsset(
        control: imports,
        prepare: _prepare,
        selectedPath: 'other.bin',
        export: _export,
      ),
      throwsStateError,
    );
    expect(imports.writes, 0);
    control.hold!.complete();
    await first;
    expect(imports.writes, 2);
    expect(owner.importing, isFalse);
  });

  test(
    'Ready selection belongs to root before a detached view receives it',
    () async {
      final owner = _owner(_Drafts());
      final view = await owner.importAsset(
        control: _Imports(),
        prepare: _prepare,
        selectedPath: 'selected.bin',
        export: _export,
      );
      expect(owner.session.current.assets.single.assetId, 'asset-one');
      expect(owner.session.current.values.description.text, 'raw body');
      expect(
        owner
            .metadata(category: 'category', stage: 'stage', selected: const [])
            .assets
            .single
            .assetId,
        'asset-one',
      );
      owner.metadata(
        category: 'category',
        stage: 'stage',
        selected: [view.attachment],
      );
      // Removal becomes meaningful only after the view has received the selection.
      expect(
        owner
            .metadata(category: 'category', stage: 'stage', selected: const [])
            .assets,
        isEmpty,
      );
    },
  );

  test(
    'Ready export failure cannot be cleared by ordinary raw capture',
    () async {
      final owner = _owner(_Drafts());
      final imports = _Imports();
      await expectLater(
        owner.importAsset(
          control: imports,
          prepare: _prepare,
          selectedPath: 'selected.bin',
          export: (_) async => throw StateError('export failed'),
        ),
        throwsStateError,
      );
      expect(
        owner.imports.single.confirmed!.record!.phase,
        EditorDraftImportPhase.ready,
      );
      expect(owner.hasUnresolvedImports, isTrue);
      expect(
        () => owner.metadata(
          category: 'category',
          stage: 'stage',
          selected: const [],
        ),
        throwsStateError,
      );
      await expectLater(
        owner.workspace.prepareClose(timeout: const Duration(seconds: 1)),
        throwsStateError,
      );
      expect(owner.session.disposed, isFalse);
      expect(imports.writes, 2);
    },
  );
}
