import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';

EditorDraftAssetSelection asset(
  EditorDraftAssetOrigin origin,
  String id, [
  List<String> aliases = const [],
]) => EditorDraftAssetSelection(origin: origin, assetId: id, aliases: aliases);

EditorDraftSnapshot snapshot(
  String title,
  List<EditorDraftAssetSelection> assets,
) {
  EditorDraftTextValue text(String value) => EditorDraftTextValue(
    text: value,
    selectionBase: -1,
    selectionExtent: -1,
    affinity: 1,
    directional: false,
    composingStart: -1,
    composingEnd: -1,
  );
  return EditorDraftSnapshot(
    values: EditorDraftValues(
      title: text(title),
      description: text(''),
      hypothesis: text(''),
      conclusion: text(''),
      todos: text(''),
      category: '灵感',
      stage: '待整理',
    ),
    assets: assets,
  );
}

EditorDraftRecord receipt(
  EditorDraftWriteRequest request, {
  BigInt? currentGeneration,
  bool pins = true,
}) {
  final generation = request.expectedGeneration + BigInt.one;
  return EditorDraftRecord(
    request: request,
    generation: generation,
    active: true,
    currentGeneration: currentGeneration ?? generation,
    currentActive: true,
    repeated: false,
    sourceFormat: 2,
    sourceRevision: request.sourceRevision,
    sourceSha256: List.filled(32, 1),
    predecessorRevision: BigInt.zero,
    predecessorSha256: const [],
    assets: pins
        ? [
            for (final selected in request.assets)
              EditorDraftStoredAsset(
                selection: selected,
                name: selected.assetId,
                mediaType: 'image/*',
                bytes: BigInt.one,
                sha256: List.filled(32, 2),
              ),
          ]
        : const [],
  );
}

final class PendingSave {
  PendingSave(this.request) : result = Completer<EditorDraftRecord>();
  final EditorDraftWriteRequest request;
  final Completer<EditorDraftRecord> result;
}

final class Control implements EditorDraftControl {
  final saves = <PendingSave>[];
  @override
  Future<EditorDraftRecord> save(EditorDraftWriteRequest request) {
    final pending = PendingSave(request);
    saves.add(pending);
    return pending.result.future;
  }

  @override
  Future<EditorDraftRecord?> read(String cardId, String draftId) async => null;
  @override
  Future<List<EditorDraftSummary>> list() async => const [];
  @override
  Future<EditorDraftRecord> discard(
    String cardId,
    String draftId,
    BigInt expectedGeneration,
    String operation,
  ) => throw UnimplementedError();
  @override
  Future<EditorDraftImportedAsset> importAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String path,
    String name,
    String kind,
    BigInt bytes,
  ) => throw UnimplementedError();
  @override
  Future<void> exportAsset(
    String cardId,
    String draftId,
    BigInt generation,
    String assetId,
    String path,
  ) => throw UnimplementedError();
}

EditorDraftSession session(
  Control control, {
  Duration? debounce,
  bool Function()? isCurrent,
}) {
  var serial = 0;
  return EditorDraftSession.newSession(
    control: control,
    cardId: 'card',
    draftId: 'draft',
    sourceRevision: BigInt.one,
    initialSnapshot: snapshot('initial', const []),
    operationFactory: () => 'draft-${++serial}',
    isCurrent: isCurrent ?? () => true,
    debounce: debounce,
  );
}

Future<void> tick() => Future<void>.delayed(Duration.zero);

void main() {
  test(
    'adoption preserves S2 order, aliases, new assets and frozen request',
    () async {
      final control = Control();
      final draft = session(control);
      addTearDown(draft.dispose);
      draft.observe(
        snapshot('S1', [
          asset(EditorDraftAssetOrigin.staged, 'a', ['S1']),
          asset(EditorDraftAssetOrigin.source, 'b'),
        ]),
      );
      final first = draft.flush();
      await tick();
      final frozen = control.saves.single.request;
      draft.observe(
        snapshot('S2', [
          asset(EditorDraftAssetOrigin.staged, 'c', ['new']),
          asset(EditorDraftAssetOrigin.staged, 'a', ['S2']),
        ]),
      );
      final local = draft.localGeneration;
      final confirmed = receipt(frozen);
      control.saves.single.result.complete(confirmed);
      await first;
      draft.adoptConfirmedAssetPins(confirmed);
      draft.adoptConfirmedAssetPins(confirmed);
      expect(draft.localGeneration, local);
      expect(draft.current.values.title.text, 'S2');
      expect(draft.current.assets.map((a) => a.assetId), ['c', 'a']);
      expect(draft.current.assets.map((a) => a.origin), [
        EditorDraftAssetOrigin.staged,
        EditorDraftAssetOrigin.previousDraft,
      ]);
      expect(draft.current.assets.last.aliases, ['S2']);
      expect(frozen.assets.first.origin, EditorDraftAssetOrigin.staged);
      expect(draft.dirty, isTrue);
      expect(control.saves, hasLength(1));
      final next = draft.flush();
      await tick();
      expect(
        control.saves.last.request.assets.last.origin,
        EditorDraftAssetOrigin.previousDraft,
      );
      control.saves.last.result.complete(receipt(control.saves.last.request));
      await next;
    },
  );

  test('matching adoption without S2 remains clean', () async {
    final control = Control();
    final draft = session(control);
    addTearDown(draft.dispose);
    draft.observe(snapshot('S1', [asset(EditorDraftAssetOrigin.staged, 'a')]));
    final first = draft.flush();
    await tick();
    final confirmed = receipt(control.saves.single.request);
    control.saves.single.result.complete(confirmed);
    await first;
    final local = draft.localGeneration;
    draft.adoptConfirmedAssetPins(confirmed);
    expect(
      draft.current.assets.single.origin,
      EditorDraftAssetOrigin.previousDraft,
    );
    expect(draft.localGeneration, local);
    expect(draft.dirty, isFalse);
    expect(await draft.flush(), same(confirmed));
    expect(control.saves, hasLength(1));
  });

  test(
    'foreign, malformed, in-flight and stale receipts are rejected',
    () async {
      final control = Control();
      var current = true;
      final draft = session(control, isCurrent: () => current);
      addTearDown(draft.dispose);
      draft.observe(
        snapshot('S1', [asset(EditorDraftAssetOrigin.staged, 'a')]),
      );
      final first = draft.flush();
      await tick();
      final request = control.saves.single.request;
      final confirmed = receipt(request);
      expect(() => draft.adoptConfirmedAssetPins(confirmed), throwsStateError);
      control.saves.single.result.complete(confirmed);
      await first;
      expect(
        () => draft.adoptConfirmedAssetPins(receipt(request)),
        throwsStateError,
      );
      current = false;
      expect(() => draft.adoptConfirmedAssetPins(confirmed), throwsStateError);

      final other = session(control);
      addTearDown(other.dispose);
      other.observe(
        snapshot('S1', [asset(EditorDraftAssetOrigin.staged, 'a')]),
      );
      final saving = other.flush();
      await tick();
      control.saves.last.result.complete(
        receipt(control.saves.last.request, pins: false),
      );
      await saving;
      expect(
        () => other.adoptConfirmedAssetPins(other.confirmed!),
        throwsFormatException,
      );
    },
  );

  test('unknown and historical receipts cannot adopt', () async {
    final control = Control();
    final draft = session(control);
    addTearDown(draft.dispose);
    draft.observe(snapshot('S1', [asset(EditorDraftAssetOrigin.staged, 'a')]));
    final first = draft.flush();
    await tick();
    final request = control.saves.single.request;
    final failed = expectLater(first, throwsA(isA<EditorDraftSaveFailure>()));
    control.saves.single.result.completeError(StateError('lost reply'));
    await failed;
    expect(draft.unknown, isTrue);
    expect(
      () => draft.adoptConfirmedAssetPins(receipt(request)),
      throwsStateError,
    );

    final other = session(control);
    addTearDown(other.dispose);
    other.observe(snapshot('S1', [asset(EditorDraftAssetOrigin.staged, 'a')]));
    final second = other.flush();
    await tick();
    final conflict = expectLater(
      second,
      throwsA(isA<EditorDraftSaveFailure>()),
    );
    final historical = receipt(
      control.saves.last.request,
      currentGeneration: BigInt.two,
    );
    control.saves.last.result.complete(historical);
    await conflict;
    expect(other.conflicted, isTrue);
    expect(() => other.adoptConfirmedAssetPins(historical), throwsStateError);
  });

  test('noCommit after in-flight S2 schedules only a new snapshot', () async {
    final control = Control();
    final draft = session(control, debounce: const Duration(milliseconds: 10));
    addTearDown(draft.dispose);
    draft.observe(snapshot('S1', const []));
    final first = draft.flush();
    await tick();
    final original = control.saves.single.request;
    draft.observe(snapshot('S2', const []));
    final failed = expectLater(first, throwsA(isA<EditorDraftSaveFailure>()));
    control.saves.single.result.completeError(
      EditorDraftSaveFailure(
        request: original,
        outcomeUnknown: false,
        cause: StateError('rejected'),
      ),
    );
    await failed;
    await Future<void>.delayed(const Duration(milliseconds: 60));
    expect(control.saves, hasLength(2));
    expect(control.saves.last.request.values.title.text, 'S2');
    expect(control.saves.last.request.expectedGeneration, BigInt.zero);
    final newer = control.saves.last;
    final newerFailed = expectLater(
      draft.flush(),
      throwsA(isA<EditorDraftSaveFailure>()),
    );
    newer.result.completeError(
      EditorDraftSaveFailure(
        request: newer.request,
        outcomeUnknown: false,
        cause: StateError('rejected'),
      ),
    );
    await newerFailed;
    await Future<void>.delayed(const Duration(milliseconds: 60));
    expect(control.saves, hasLength(2));
  });
}
