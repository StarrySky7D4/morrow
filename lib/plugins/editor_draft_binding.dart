import 'package:flutter/widgets.dart';

import 'editor_draft.dart';
import 'editor_draft_session.dart';

/// Only already resolved draft asset selections belong here. A caller must
/// surface a pending/failed import instead of omitting an unresolved selection.
final class EditorDraftMetadata {
  EditorDraftMetadata({
    required this.category,
    required this.stage,
    required List<EditorDraftAssetSelection> assets,
  }) : assets = List.unmodifiable(assets);
  final String category, stage;
  final List<EditorDraftAssetSelection> assets;
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
    if (_disposed || session.disposed) {
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
    try {
      if (!(isCurrent?.call() ?? true)) {
        throw StateError('The draft view belongs to an inactive workspace');
      }
      final metadata = readMetadata();
      final snapshot = EditorDraftSnapshot(
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
      );
      session.observe(snapshot);
      final changed = _uncaptured || _captureFailure != null;
      _uncaptured = false;
      _captureFailure = null;
      if (changed) notifyListeners();
      return true;
    } catch (error) {
      _uncaptured = true;
      _captureFailure = error;
      notifyListeners();
      return false;
    }
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

  /// Explicitly restore the session's current raw values. Validate every field
  /// before changing the view; suppress listeners until the whole view is set.
  /// [applyMetadata] must synchronously apply the supplied selection as a unit.
  void applyCurrentToView(void Function(EditorDraftMetadata) applyMetadata) {
    if (_disposed) throw StateError('The draft binding was disposed');
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
    );
    _applying = true;
    try {
      applyMetadata(metadata);
      final controllers = _controllers;
      for (var i = 0; i < controllers.length; i++) {
        controllers[i].value = values[i];
      }
      _captureFailure = null;
      _uncaptured = false;
    } finally {
      _applying = false;
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
