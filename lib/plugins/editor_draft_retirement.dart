import 'editor_draft.dart';
import 'editor_draft_codec.dart';
import 'editor_draft_import.dart';

/// One explicit retirement of an exact, reviewed journal. Inspection may advance
/// a never-started cleanup stage, but never repeats a submitted mutation.
final class EditorDraftRetirementAttempt {
  EditorDraftRetirementAttempt({
    required this.original,
    required this.operation,
    required this._control,
    required this._imports,
    required this._isCurrent,
    this._reserveOwner,
    this._finishOwner,
  }) {
    if (!original.active ||
        !original.currentActive ||
        original.repeated ||
        original.generation != original.currentGeneration ||
        original.requestSha256.length != 32 ||
        operation.isEmpty ||
        operation == original.request.operation ||
        operation == original.request.commitOperation) {
      throw StateError(
        'An exact active draft and a new retirement are required',
      );
    }
  }

  final EditorDraftRecord original;
  final String operation;
  final EditorDraftControl _control;
  final EditorDraftImportControl _imports;
  final bool Function() _isCurrent;
  final void Function()? _reserveOwner;
  final Future<void> Function(EditorDraftRecord)? _finishOwner;
  Future<void>? _ownerFinish;
  Future<EditorDraftRecord>? _start;
  Future<EditorDraftRecord>? _inspection;
  EditorDraftRecord? _confirmed;
  Object? _firstFailure;
  bool _dispatched = false;
  bool _cleanupDispatched = false;

  EditorDraftRecord? get confirmed => _confirmed;
  Object? get firstFailure => _firstFailure;
  bool get unknown => _dispatched && _confirmed == null;
  bool get dispatched => _dispatched;
  bool get started => _start != null;

  void _current() {
    if (!_isCurrent()) {
      throw StateError('Original retirement workspace changed');
    }
  }

  bool _sameBytes(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  bool _sameRequest(EditorDraftRecord actual) =>
      _sameBytes(actual.requestSha256, original.requestSha256) &&
      _sameBytes(
        EditorDraftCodec.encodeWrite(actual.request),
        EditorDraftCodec.encodeWrite(original.request),
      );

  EditorDraftRecord _accept(EditorDraftRecord? actual) {
    _current();
    if (actual == null ||
        actual.active ||
        actual.currentActive ||
        actual.generation != original.generation + BigInt.one ||
        actual.currentGeneration != actual.generation ||
        actual.discardOperation != operation ||
        actual.retirement != null ||
        !_sameRequest(actual)) {
      throw StateError('The original retirement is not confirmed');
    }
    return actual;
  }

  Future<EditorDraftRecord> start() => _start ??= _run().catchError((Object e) {
    _firstFailure ??= e;
    throw e;
  });

  Future<EditorDraftRecord> _run() async {
    _current();
    _reserveOwner?.call();
    final request = original.request;
    final actual = await _control.read(request.cardId, request.draftId);
    _current();
    if (actual == null ||
        !actual.active ||
        !actual.currentActive ||
        actual.generation != original.generation ||
        actual.currentGeneration != original.generation ||
        !_sameRequest(actual)) {
      throw StateError('The reviewed draft changed before retirement');
    }
    _dispatched = true;
    return _finish(
      _accept(
        await _control.discard(
          request.cardId,
          request.draftId,
          original.generation,
          operation,
        ),
      ),
    );
  }

  Future<EditorDraftRecord> inspect() {
    if (!_dispatched) {
      return Future.error(StateError('No original retirement was submitted'));
    }
    return _inspection ??= _inspect()
        .catchError((Object e) {
          _firstFailure ??= e;
          throw e;
        })
        .whenComplete(() => _inspection = null);
  }

  Future<EditorDraftRecord> _inspect() async {
    _current();
    return _finish(
      _accept(
        await _control.read(original.request.cardId, original.request.draftId),
      ),
    );
  }

  Future<EditorDraftRecord> _finish(EditorDraftRecord retired) async {
    _current();
    // Cleanup is a distinct stage. A lost discard reply can prove the original
    // journal first and then dispatch this never-started stage. A lost cleanup
    // reply is only inspected; the cleanup mutation is not submitted twice.
    final request = original.request;
    if (!_cleanupDispatched) {
      _cleanupDispatched = true;
      await _imports.reconcile(request.cardId, request.draftId);
      _current();
    }
    final pins = await _imports.list(request.cardId, request.draftId);
    _current();
    if (pins.kind != EditorDraftImportResultKind.list ||
        pins.cardId != request.cardId ||
        pins.draftId != request.draftId ||
        pins.currentGeneration != retired.generation ||
        pins.mainActive ||
        pins.records == null ||
        pins.records!.isNotEmpty) {
      throw StateError('Original draft import retirement is not confirmed');
    }
    // Recheck the original journal after the independent import observation.
    _accept(await _control.read(request.cardId, request.draftId));
    if (_finishOwner case final finish?) {
      await (_ownerFinish ??= Future<void>.sync(() => finish(retired)));
      _current();
    }
    return _confirmed = retired;
  }
}
