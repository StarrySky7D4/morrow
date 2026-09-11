/// Host widgets for validated UI descriptions. Intents are local routing data, not commits.
library;

import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:morrow_core_client/ui_models.dart';
export 'package:morrow_core_client/ui_models.dart'
    show UiDocumentModel, UiNode, Kind, Tone, EventKind;

@immutable
final class UiIntent {
  const UiIntent({
    required this.node,
    required this.action,
    required this.kind,
    this.text = '',
    this.checked = false,
  });
  final String node, action, text;
  final EventKind kind;
  final bool checked;
}

/// [viewIdentity] must change when the host opens a different view/generation.
/// [onIntent] stays in Dart; the host owns acknowledgement, serials, task routing and persistence.
class PluginForm extends StatefulWidget {
  const PluginForm({
    super.key,
    required this.document,
    required this.viewIdentity,
    required this.onIntent,
  });
  final UiDocumentModel document;
  final Object viewIdentity;
  final ValueChanged<UiIntent> onIntent;
  @override
  State<PluginForm> createState() => _PluginFormState();
}

class _PluginFormState extends State<PluginForm> {
  int _epoch = 0;
  late Map<String, UiNode> _nodes;
  late Map<String, List<UiNode>> _children;
  @override
  void initState() {
    super.initState();
    _index();
  }

  @override
  void didUpdateWidget(covariant PluginForm oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.viewIdentity != widget.viewIdentity) _epoch++;
    _index();
  }

  void _index() {
    _nodes = {for (final n in widget.document.nodes) n.id: n};
    _children = {};
    for (final n in widget.document.nodes) {
      _children.putIfAbsent(n.parent, () => []).add(n);
    }
  }

  void _emit(
    UiNode original,
    int epoch,
    EventKind kind, {
    String text = '',
    bool checked = false,
  }) {
    if (!mounted || epoch != _epoch) return;
    final current = _nodes[original.id];
    if (current == null ||
        !current.enabled ||
        current.kind != original.kind ||
        current.action != original.action) {
      return;
    }
    if (kind == EventKind.editText && !_validText(text, current.maxBytes)) {
      return;
    }
    widget.onIntent(
      UiIntent(
        node: current.id,
        action: current.action,
        kind: kind,
        text: text,
        checked: checked,
      ),
    );
  }

  Widget _node(UiNode n) {
    final epoch = _epoch;
    final key = ValueKey((widget.viewIdentity, n.id, n.kind));
    final children = _children[n.id] ?? const <UiNode>[];
    switch (n.kind) {
      case Kind.column:
        return Column(
          key: key,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < children.length; i++) ...[
              if (i > 0) const SizedBox(height: 12),
              _node(children[i]),
            ],
          ],
        );
      case Kind.row:
        return LayoutBuilder(
          key: key,
          builder: (context, c) {
            if (children.isEmpty) return const SizedBox.shrink();
            final width = c.hasBoundedWidth ? c.maxWidth : 280.0;
            final columns = (width / 260).floor().clamp(1, children.length);
            final slot = ((width - (columns - 1) * 12) / columns).clamp(
              0.0,
              width,
            );
            return Wrap(
              spacing: 12,
              runSpacing: 12,
              children: [
                for (final child in children)
                  SizedBox(width: slot, child: _node(child)),
              ],
            );
          },
        );
      case Kind.text:
        final theme = Theme.of(context);
        final color = switch (n.tone) {
          Tone.normal => theme.colorScheme.onSurface,
          Tone.muted => theme.colorScheme.onSurfaceVariant,
          Tone.emphasis => theme.colorScheme.primary,
        };
        return Text(
          n.text,
          key: key,
          softWrap: true,
          style: theme.textTheme.bodyLarge?.copyWith(
            color: color,
            fontWeight: n.tone == Tone.emphasis ? FontWeight.w600 : null,
          ),
        );
      case Kind.button:
        return Align(
          key: key,
          alignment: AlignmentDirectional.centerStart,
          child: FilledButton(
            onPressed: n.enabled
                ? () => _emit(n, epoch, EventKind.activate)
                : null,
            child: Text(n.label, softWrap: true),
          ),
        );
      case Kind.textInput:
        return _Input(
          key: key,
          node: n,
          onText: (text) => _emit(n, epoch, EventKind.editText, text: text),
        );
      case Kind.toggle:
        return _Toggle(
          key: key,
          node: n,
          onToggle: (value) =>
              _emit(n, epoch, EventKind.setToggle, checked: value),
        );
    }
  }

  @override
  Widget build(BuildContext context) => Material(
    type: MaterialType.transparency,
    child: FocusTraversalGroup(
      child: SingleChildScrollView(
        // Floating input labels extend above their border; leave room before
        // clipping the scrolling viewport so the first label remains readable.
        padding: const EdgeInsets.symmetric(vertical: 8),
        primary: false,
        child: _node(widget.document.nodes.first),
      ),
    ),
  );
}

bool _validText(String text, int maxBytes) =>
    utf8.encode(text).length <= maxBytes &&
    !RegExp(r'[\x00-\x08\x0b-\x1f\x7f-\x9f]').hasMatch(text);

class _Input extends StatefulWidget {
  const _Input({super.key, required this.node, required this.onText});
  final UiNode node;
  final ValueChanged<String> onText;
  @override
  State<_Input> createState() => _InputState();
}

class _InputState extends State<_Input> {
  late final TextEditingController _controller;
  late TextEditingValue _lastValid;
  late String _emitted;
  bool _applying = false;
  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.node.text);
    _lastValid = _controller.value;
    _emitted = widget.node.text;
    _controller.addListener(_edited);
  }

  @override
  void didUpdateWidget(covariant _Input oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.node.text != widget.node.text ||
        oldWidget.node.maxBytes != widget.node.maxBytes) {
      // Only explicit field-value/budget changes replace the local editing buffer.
      _applying = true;
      final text = widget.node.text;
      _controller.value = TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(
          offset: _controller.selection.extentOffset.clamp(0, text.length),
        ),
      );
      _lastValid = _controller.value;
      _emitted = text;
      _applying = false;
    }
  }

  void _edited() {
    if (_applying || !mounted) return;
    final value = _controller.value;
    if (value.composing.isValid && !value.composing.isCollapsed) return;
    if (!_validText(value.text, widget.node.maxBytes)) return;
    _lastValid = value;
    if (value.text != _emitted) {
      _emitted = value.text;
      widget.onText(value.text);
    }
  }

  TextEditingValue _format(TextEditingValue oldValue, TextEditingValue next) {
    if (next.composing.isValid && !next.composing.isCollapsed) return next;
    if (_validText(next.text, widget.node.maxBytes)) return next;
    return _lastValid.copyWith(composing: TextRange.empty);
  }

  @override
  Widget build(BuildContext context) => TextField(
    controller: _controller,
    enabled: widget.node.enabled,
    decoration: InputDecoration(labelText: widget.node.label),
    minLines: 1,
    maxLines: 4,
    inputFormatters: [TextInputFormatter.withFunction(_format)],
  );
  @override
  void dispose() {
    _controller.removeListener(_edited);
    _controller.dispose();
    super.dispose();
  }
}

class _Toggle extends StatefulWidget {
  const _Toggle({super.key, required this.node, required this.onToggle});
  final UiNode node;
  final ValueChanged<bool> onToggle;
  @override
  State<_Toggle> createState() => _ToggleState();
}

class _ToggleState extends State<_Toggle> {
  late bool _value;
  @override
  void initState() {
    super.initState();
    _value = widget.node.checked;
  }

  @override
  void didUpdateWidget(covariant _Toggle oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.node.checked != widget.node.checked) {
      _value = widget.node.checked;
    }
  }

  @override
  Widget build(BuildContext context) => SwitchListTile.adaptive(
    contentPadding: EdgeInsets.zero,
    title: Text(widget.node.label),
    value: _value,
    onChanged: widget.node.enabled
        ? (value) {
            setState(() => _value = value);
            widget.onToggle(value);
          }
        : null,
  );
}
