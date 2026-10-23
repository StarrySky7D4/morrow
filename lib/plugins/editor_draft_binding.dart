import 'package:flutter/widgets.dart';

import 'editor_draft.dart';
import 'editor_draft_session.dart';
import 'editor_draft_workspace.dart';

/// Only already resolved draft asset selections belong here. A caller must
/// surface a pending/failed import instead of omitting an unresolved selection.
final class EditorDraftMetadata {
  EditorDraftMetadata({
    required this.category,
    required this.stage,
    required List<EditorDraftAssetSelection> assets,
    this.captureScope = '',
    this.captureEvents = 0,
    this.incomplete,
  }) : assets = List.unmodifiable(assets);
  final String category, stage;
  final List<EditorDraftAssetSelection> assets;
  final String captureScope;
  final int captureEvents;
  final Object? incomplete;
}

/// Attaches a replaceable view to a longer-lived draft session. Attaching never
/// applies old text or writes a draft. Restoring a view is a separate decision.
final class EditorDraftBinding extends ChangeNotifier {
  EditorDraftBinding({
    required this.session,
    required this.title,
    required this.description,
    required this.hypothesis,
    required this.conclusion,
    required this.todos,
    required this.readMetadata,
    this.isCurrent,
  }) {
    if (_controllers.toSet().length != 5) {
      throw ArgumentError('Draft fields require distinct controllers');
    }
  }

  final EditorDraftSession session;
  final TextEditingController title, description, hypothesis, conclusion, todos;
  final EditorDraftMetadata Function() readMetadata;
  final bool Function()? isCurrent;
  bool _attached = false, _disposed = false, _applying = false;
  bool _uncaptured = false;
  int _captureSerial = 0;
  Object? _captureFailure;

  bool get attached => _attached;
  bool get hasUncapturedChanges => _uncaptured;
  Object? get captureFailure => _captureFailure;
  List<TextEditingController> get _controllers => [
    title,
    description,
    hypothesis,
    conclusion,
    todos,
  ];

  void attach() {
    if (_disposed || session.disposed || session.retirementFrozen) {
      throw StateError('The draft binding or session was disposed');
    }
    if (_attached) return;
    _attached = true;
    for (final controller in _controllers) {
      controller.addListener(_changed);
    }
    session.attachUI();
  }

  void detach() {
    if (!_attached) return;
    _attached = false;
    for (final controller in _controllers) {
      controller.removeListener(_changed);
    }
    session.detachUI();
  }

  void _changed() {
    if (!_applying) capture();
  }

  /// A parent widget can already be unmounted during child disposal. The
  /// original prepared lease authorizes only an unchanged complete snapshot;
  /// late input, metadata failures and cancelled preparations still capture
  /// normally and therefore keep the close barrier if the view is stale.
  bool captureBeforeDetach(EditorDraftViewLease? lease) {
    if (!_disposed &&
        _attached &&
        !_applying &&
        !_uncaptured &&
        identical(lease?.session, session)) {
      try {
        final observation = _readSnapshot();
        if (observation.incomplete == null &&
            lease!.acceptsPreparedSnapshot(observation.snapshot)) {
          return true;
        }
      } catch (_) {
        // Use normal capture to retain the original incomplete state.
      }
    }
    return capture();
  }

  static EditorDraftTextValue _read(TextEditingController controller) {
    final value = controller.value;
    return EditorDraftTextValue(
      text: value.text,
      selectionBase: value.selection.baseOffset,
      selectionExtent: value.selection.extentOffset,
      affinity: value.selection.affinity.index,
      directional: value.selection.isDirectional,
      composingStart: value.composing.start,
      composingEnd: value.composing.end,
    );
  }

  /// Also call after category, stage or resolved attachment selection changes.
  /// Failure is explicit and leaves all controllers untouched; no stale subset
  /// is handed to autosave while attachment metadata cannot be captured.
  bool capture() {
    if (_disposed || !_attached || _applying) return false;
    final serial = ++_captureSerial;
    try {
      if (!(isCurrent?.call() ?? true)) {
        throw StateError('The draft view belongs to an inactive workspace');
      }
      final observation = _readSnapshot();
      if (serial != _captureSerial) return false;
      if (observation.incomplete case final failure?) {
        session.observeIncomplete(observation.snapshot, failure);
      } else {
        session.observe(observation.snapshot);
      }
      // Session listeners may synchronously capture newer input or a metadata
      // failure. Do not overwrite their result with this older observation.
      if (serial != _captureSerial) {
        return !_uncaptured && !session.captureBlocked;
      }
      if (session.captureBlocked) {
        _uncaptured = true;
        _captureFailure = session.captureFailure;
        notifyListeners();
        return false;
      }
      final changed = _uncaptured || _captureFailure != null;
      _uncaptured = false;
      _captureFailure = null;
      if (changed) notifyListeners();
      return true;
    } catch (error) {
      _markIncomplete(error);
      return false;
    }
  }

  ({EditorDraftSnapshot snapshot, Object? incomplete}) _readSnapshot() {
    final metadata = readMetadata();
    return (
      snapshot: EditorDraftSnapshot(
        values: EditorDraftValues(
          title: _read(title),
          description: _read(description),
          hypothesis: _read(hypothesis),
          conclusion: _read(conclusion),
          todos: _read(todos),
          category: metadata.category,
          stage: metadata.stage,
        ),
        assets: metadata.assets,
        captureScope: metadata.captureScope,
        captureEvents: metadata.captureEvents,
      ),
      incomplete: metadata.incomplete,
    );
  }

  void _markIncomplete(Object error) {
    _uncaptured = true;
    _captureFailure = error;
    if (!session.disposed) {
      final previous = session.current.values;
      try {
        session.observeIncompleteValues(
          EditorDraftValues(
            title: _read(title),
            description: _read(description),
            hypothesis: _read(hypothesis),
            conclusion: _read(conclusion),
            todos: _read(todos),
            category: previous.category,
            stage: previous.stage,
          ),
          error,
        );
      } catch (_) {
        // A stale workspace cannot adopt new values; it still retains the
        // unresolved capture barrier instead of throwing from a UI listener.
        session.markCaptureIncomplete(error);
      }
    }
    notifyListeners();
  }

  static TextEditingValue _value(EditorDraftTextValue raw) {
    if (raw.affinity < 0 || raw.affinity >= TextAffinity.values.length) {
      throw const FormatException('Invalid draft selection affinity');
    }
    bool valid(int offset) =>
        offset == -1 || (offset >= 0 && offset <= raw.text.length);
    if (!valid(raw.selectionBase) ||
        !valid(raw.selectionExtent) ||
        !valid(raw.composingStart) ||
        !valid(raw.composingEnd) ||
        (raw.composingStart >= 0 &&
            raw.composingEnd >= 0 &&
            raw.composingStart > raw.composingEnd)) {
      throw const FormatException('Invalid draft selection or composition');
    }
    return TextEditingValue(
      text: raw.text,
      selection: TextSelection(
        baseOffset: raw.selectionBase,
        extentOffset: raw.selectionExtent,
        affinity: TextAffinity.values[raw.affinity],
        isDirectional: raw.directional,
      ),
      composing: TextRange(start: raw.composingStart, end: raw.composingEnd),
    );
  }

  /// Validates and converts an archival raw value without attaching an owner,
  /// registering a listener or dispatching any host request.
  static TextEditingValue editingValue(EditorDraftTextValue raw) => _value(raw);

  /// Apply an original root-owned paste while suppressing only this binding's
  /// capture listener. Other UI listeners still observe the real new value.
  void applyPaste(
    String field,
    String expectedBefore,
    EditorDraftTextValue value,
  ) {
    if (_disposed || !_attached) throw StateError('Paste view is detached');
    final controller = switch (field) {
      'title' => title,
      'description' => description,
      'hypothesis' => hypothesis,
      'conclusion' => conclusion,
      'todos' => todos,
      _ => throw const FormatException('Unknown paste field'),
    };
    if (controller.text != expectedBefore) {
      throw StateError('Paste view changed');
    }
    _applying = true;
    try {
      controller.value = _value(value);
    } finally {
      _applying = false;
    }
  }

  /// Explicitly restore the session's current raw values. Validate every field
  /// before changing the view; suppress listeners until the whole view is set.
  /// [applyMetadata] must synchronously apply the supplied selection as a unit.
  void applyCurrentToView(void Function(EditorDraftMetadata) applyMetadata) {
    if (_disposed) throw StateError('The draft binding was disposed');
    try {
      if (!(isCurrent?.call() ?? true)) {
        throw StateError('The draft view belongs to an inactive workspace');
      }
      final current = session.current;
      final fields = current.values;
      final values = [
        for (final raw in [
          fields.title,
          fields.description,
          fields.hypothesis,
          fields.conclusion,
          fields.todos,
        ])
          _value(raw),
      ];
      final metadata = EditorDraftMetadata(
        category: fields.category,
        stage: fields.stage,
        assets: current.assets,
        captureScope: current.captureScope,
        captureEvents: current.captureEvents,
      );
      _applying = true;
      try {
        applyMetadata(metadata);
        final controllers = _controllers;
        for (var i = 0; i < controllers.length; i++) {
          controllers[i].value = values[i];
        }
      } finally {
        _applying = false;
      }
      // Re-read all applied state, including attachment metadata, before
      // clearing an old failure. This also supports explicit restore before
      // attaching listeners without treating a partial restore as complete.
      final restored = _readSnapshot();
      if (restored.incomplete != null || !restored.snapshot.sameAs(current)) {
        throw StateError('Restored draft differs from the complete snapshot');
      }
      _captureFailure = null;
      _uncaptured = false;
      session.observe(restored.snapshot);
      if (session.captureBlocked) {
        throw session.captureFailure ??
            StateError('Restored draft became incomplete');
      }
    } catch (error) {
      _markIncomplete(error);
      rethrow;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    if (_disposed) return;
    detach();
    _disposed = true;
    // Controllers and session belong to their independent owners.
    super.dispose();
  }
}
