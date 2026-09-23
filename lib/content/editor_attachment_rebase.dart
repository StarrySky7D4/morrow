import 'dart:collection';
import 'dart:math' as math;

import 'package:flutter/services.dart';

import '../attachments/attachment.dart';

final class _Rewrite {
  const _Rewrite(this.start, this.end, this.text);
  final int start;
  final int end;
  final String text;
}

Iterable<String> _aliases(IdeaAttachment item) sync* {
  final source = item.source;
  final values = {
    source.location,
    source.name,
    Uri.encodeComponent(source.location),
    Uri.encodeComponent(source.name),
  };
  for (final value in values) {
    if (value.isNotEmpty) yield value;
  }
}

/// Rebinds only S1-submitted local objects that remain selected in the live
/// draft. The positional S1 result is checked before any returned value is
/// constructed; a caller can then apply both returned fields together.
({List<IdeaAttachment> attachments, TextEditingValue description})
rebaseEditorAttachments({
  required List<IdeaAttachment> submitted,
  required List<IdeaAttachment> confirmed,
  required List<IdeaAttachment> selected,
  required TextEditingValue description,
}) {
  final selectedSet = HashSet<IdeaAttachment>.identity()..addAll(selected);
  final replacements = HashMap<IdeaAttachment, IdeaAttachment>.identity();
  final canonicalIds = <String>{};

  for (var index = 0; index < submitted.length; index++) {
    final original = submitted[index];
    if (original.pluginId != null ||
        !original.source.local ||
        !selectedSet.contains(original)) {
      continue;
    }
    if (replacements.containsKey(original) || index >= confirmed.length) {
      throw const FormatException('Ambiguous submitted attachment');
    }
    final canonical = confirmed[index];
    final id = canonical.pluginId;
    if (id == null ||
        id.isEmpty ||
        canonical.source.name != original.source.name ||
        canonical.source.kind != original.source.kind ||
        canonical.byteLength != original.byteLength ||
        !canonicalIds.add(id)) {
      throw const FormatException('Confirmed attachment differs from S1');
    }
    replacements[original] = canonical;
  }

  final selectedOutput = <IdeaAttachment>[
    for (final item in selected) replacements[item] ?? item,
  ];

  if (replacements.isEmpty) {
    return (
      attachments: List<IdeaAttachment>.unmodifiable(selectedOutput),
      description: description,
    );
  }

  // Raw names may be shared by different attachments. Never guess which
  // source a text URL names when an alias could identify more than one.
  final owners = <String, Set<IdeaAttachment>>{};
  for (final item in [...submitted, ...selected]) {
    for (final alias in _aliases(item)) {
      (owners[alias] ??= HashSet<IdeaAttachment>.identity()).add(item);
    }
  }

  final candidates = <_Rewrite>[];
  for (final entry in replacements.entries) {
    final id = entry.value.pluginId!;
    for (final alias in _aliases(entry.key)) {
      final pattern = RegExp(
        'attachment:${RegExp.escape(alias)}'
        r'(?=[\s)>]|$)',
      );
      for (final match in pattern.allMatches(description.text)) {
        if (owners[alias]!.length != 1) {
          throw const FormatException('Ambiguous attachment URL');
        }
        candidates.add(_Rewrite(match.start, match.end, 'attachment:$id'));
      }
    }
  }
  candidates.sort((a, b) {
    final byStart = a.start.compareTo(b.start);
    return byStart != 0 ? byStart : b.end.compareTo(a.end);
  });

  final edits = <_Rewrite>[];
  for (final candidate in candidates) {
    if (edits.isNotEmpty && candidate.start < edits.last.end) {
      final prior = edits.last;
      if (candidate.start == prior.start &&
          candidate.end <= prior.end &&
          candidate.text == prior.text) {
        continue; // A shorter alias for the same URL and same asset.
      }
      throw const FormatException('Overlapping attachment URLs');
    }
    edits.add(candidate);
  }
  if (edits.isEmpty) {
    return (
      attachments: List<IdeaAttachment>.unmodifiable(selectedOutput),
      description: description,
    );
  }

  final changed = StringBuffer();
  var cursor = 0;
  for (final edit in edits) {
    changed.write(description.text.substring(cursor, edit.start));
    changed.write(edit.text);
    cursor = edit.end;
  }
  changed.write(description.text.substring(cursor));

  int moved(int offset) {
    if (offset < 0) return offset; // Flutter's invalid selection/composition.
    var delta = 0;
    for (final edit in edits) {
      if (offset < edit.start) break;
      if (offset == edit.end) {
        return edit.start + delta + edit.text.length;
      }
      if (offset < edit.end) {
        return edit.start +
            delta +
            math.min(offset - edit.start, edit.text.length);
      }
      delta += edit.text.length - (edit.end - edit.start);
    }
    return offset + delta;
  }

  final selection = description.selection;
  final composing = description.composing;
  return (
    attachments: List<IdeaAttachment>.unmodifiable(selectedOutput),
    description: TextEditingValue(
      text: changed.toString(),
      selection: TextSelection(
        baseOffset: moved(selection.baseOffset),
        extentOffset: moved(selection.extentOffset),
        affinity: selection.affinity,
        isDirectional: selection.isDirectional,
      ),
      composing: TextRange(
        start: moved(composing.start),
        end: moved(composing.end),
      ),
    ),
  );
}
