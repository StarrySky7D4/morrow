import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'appearance.dart';
import 'tip_preferences.dart';
import 'hold_reorder.dart';

/// A page-local draft. Stable row identities keep focus with the same item
/// when it moves; nothing is persisted until the page's Apply action.
class TipListEditor extends StatefulWidget {
  const TipListEditor({
    super.key,
    required this.initial,
    required this.defaults,
    required this.onChanged,
    required this.daily,
    this.caption,
    this.defaultCaption,
    this.enabled = true,
    this.error,
    this.title,
    this.description,
    this.allowReset = true,
    this.syncInitial = false,
    this.maxTextLength,
    this.onSelection,
  });
  final List<TipItem> initial, defaults;
  final void Function(List<TipItem>? items, String? caption) onChanged;
  final bool daily, enabled;
  final String? caption, defaultCaption, error;
  final String? title, description;
  final bool allowReset, syncInitial;
  final int? maxTextLength;
  final void Function(String id, TextEditingValue value)? onSelection;
  @override
  State<TipListEditor> createState() => _TipListEditorState();
}

class _TipListEditorState extends State<TipListEditor> {
  late List<TipItem> items = [...widget.initial];
  late String caption = widget.caption ?? widget.defaultCaption ?? '';
  late bool customCaption = widget.caption != null;
  final rowKeys = <String, GlobalKey<_TipRowState>>{};
  Object orderRevision = Object();
  @override
  void didUpdateWidget(TipListEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.syncInitial && !identical(oldWidget.initial, widget.initial)) {
      items = [...widget.initial];
      orderRevision = Object();
    }
  }

  bool get zh => Localizations.localeOf(context).languageCode == 'zh';
  void change(VoidCallback edit) {
    setState(() {
      edit();
      orderRevision = Object();
    });
    widget.onChanged(List.unmodifiable(items), customCaption ? caption : null);
  }

  void move(int index, int delta) => change(() {
    final item = items.removeAt(index);
    items.insert(index + delta, item);
  });

  void add() {
    final item = TipItem.empty();
    change(() => items.add(item));
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      final row = rowKeys[item.id];
      final target = row?.currentContext;
      if (!mounted || target == null) return;
      await Scrollable.ensureVisible(
        target,
        alignment: .35,
        duration: motionDuration(context, 220),
      );
      if (mounted) row?.currentState?.focusNode.requestFocus();
    });
  }

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Container(
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: p.accent.withValues(alpha: .12),
                borderRadius: BorderRadius.circular(14),
              ),
              child: Icon(
                widget.daily
                    ? Icons.checklist_rounded
                    : Icons.format_quote_rounded,
                color: p.accent,
                size: 23,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                widget.title ??
                    (widget.daily
                        ? (zh ? '小事清单' : 'Little things')
                        : (zh ? '提示文案' : 'Your tips')),
                style: TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                  color: p.ink,
                ),
              ),
            ),
            Text(
              '${items.length} / ${TipPreferences.maxLines}',
              style: TextStyle(color: p.muted, fontSize: 12),
            ),
          ],
        ),
        const SizedBox(height: 12),
        Text(
          widget.description ??
              (widget.daily
                  ? (zh
                        ? '把此刻想做的小事留在这里。修改或排序不会打乱已完成状态。'
                        : 'Make room for little things. Editing and reordering preserve completion.')
                  : (zh
                        ? '一句一张卡片，按顺序轮换。写下你想看见的话。'
                        : 'One thought per card, shown in order. Make these words your own.')),
          style: TextStyle(color: p.muted, height: 1.6, fontSize: 12),
        ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            FilledButton.tonalIcon(
              key: const ValueKey('tip-add'),
              onPressed:
                  widget.enabled &&
                      items.length < TipPreferences.maxLines &&
                      (widget.maxTextLength == null ||
                          items
                                  .map((e) => e.text)
                                  .join('\n')
                                  .characters
                                  .length <
                              widget.maxTextLength!)
                  ? add
                  : null,
              icon: const Icon(Icons.add_rounded, size: 18),
              label: Text(zh ? '添加一条' : 'Add a tip'),
            ),
            if (widget.allowReset)
              TextButton.icon(
                key: const ValueKey('tip-text-reset'),
                onPressed: widget.enabled
                    ? () {
                        setState(() {
                          items = [...widget.defaults];
                          caption = widget.defaultCaption ?? '';
                          customCaption = false;
                          orderRevision = Object();
                        });
                        widget.onChanged(null, null);
                      }
                    : null,
                icon: const Icon(Icons.restore_rounded, size: 18),
                label: Text(zh ? '恢复默认' : 'Restore defaults'),
              ),
          ],
        ),
        const SizedBox(height: 12),
        if (items.isEmpty)
          Padding(
            padding: const EdgeInsets.all(20),
            child: Text(
              zh
                  ? '还没有文案。添加一条，或直接保存以恢复默认。'
                  : 'Add a tip, or save an empty list to restore defaults.',
              style: TextStyle(color: p.muted, height: 1.6),
            ),
          ),
        for (final (index, item) in items.indexed)
          HoldReorder(
            key: ValueKey(item.id),
            scope: this,
            id: item.id,
            revision: orderRevision,
            label: item.text,
            enabled: widget.enabled,
            onMove: (source, target, after) => change(() {
              final byId = {for (final entry in items) entry.id: entry};
              items = [
                for (final id in moveRelative(
                  items.map((e) => e.id).toList(),
                  source as String,
                  target as String,
                  after,
                ))
                  byId[id]!,
              ];
            }),
            builder: (handle) => Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: _TipRow(
                key: rowKeys.putIfAbsent(
                  item.id,
                  () => GlobalKey<_TipRowState>(),
                ),
                item: item,
                enabled: widget.enabled,
                label:
                    '${widget.daily ? (zh ? '小事' : 'Task') : (zh ? '提示' : 'Tip')} ${(index + 1).toString().padLeft(2, '0')}',
                fieldKey: ValueKey('tip-item-$index'),
                maxLength: widget.maxTextLength == null
                    ? TipPreferences.maxLength
                    : (widget.maxTextLength! -
                              items
                                  .where((e) => e.id != item.id)
                                  .fold<int>(
                                    0,
                                    (n, e) => n + e.text.characters.length,
                                  ) -
                              (items.length - 1))
                          .clamp(0, widget.maxTextLength!),
                onSelection: widget.onSelection == null
                    ? null
                    : (value) => widget.onSelection!(item.id, value),
                onChanged: (text) =>
                    change(() => items[index] = item.withText(text)),
                actions: [
                  handle,
                  IconButton(
                    key: ValueKey('tip-up-$index'),
                    tooltip: zh ? '上移' : 'Move up',
                    onPressed: widget.enabled && index > 0
                        ? () => move(index, -1)
                        : null,
                    icon: const Icon(Icons.arrow_upward_rounded, size: 17),
                  ),
                  IconButton(
                    key: ValueKey('tip-down-$index'),
                    tooltip: zh ? '下移' : 'Move down',
                    onPressed: widget.enabled && index < items.length - 1
                        ? () => move(index, 1)
                        : null,
                    icon: const Icon(Icons.arrow_downward_rounded, size: 17),
                  ),
                  IconButton(
                    key: ValueKey('tip-remove-$index'),
                    tooltip: zh ? '删除此条' : 'Remove this tip',
                    onPressed: widget.enabled
                        ? () => change(() => items.removeAt(index))
                        : null,
                    icon: const Icon(Icons.close_rounded, size: 17),
                  ),
                ],
              ),
            ),
          ),
        if (widget.daily) ...[
          const SizedBox(height: 8),
          _TipRow(
            item: TipItem('caption', caption),
            enabled: widget.enabled,
            label: zh ? '底部短句' : 'Closing note',
            fieldKey: const ValueKey('tip-daily-caption'),
            onChanged: (text) => change(() {
              caption = text;
              customCaption = true;
            }),
          ),
        ],
        const SizedBox(height: 8),
        Text(
          zh
              ? '空白条目会忽略 · 保存后生效'
              : 'Blank cards are ignored · Changes apply on save',
          style: TextStyle(fontSize: 11, color: p.muted),
        ),
        if (widget.error != null)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: Text(
              widget.error!,
              key: const ValueKey('tip-save-error'),
              style: TextStyle(
                color: Theme.of(context).colorScheme.error,
                height: 1.5,
              ),
            ),
          ),
      ],
    );
  }
}

class _TipRow extends StatefulWidget {
  const _TipRow({
    super.key,
    required this.item,
    required this.label,
    required this.fieldKey,
    required this.onChanged,
    required this.enabled,
    this.actions = const [],
    this.maxLength = TipPreferences.maxLength,
    this.onSelection,
  });
  final TipItem item;
  final String label;
  final Key fieldKey;
  final ValueChanged<String> onChanged;
  final bool enabled;
  final List<Widget> actions;
  final int maxLength;
  final ValueChanged<TextEditingValue>? onSelection;
  @override
  State<_TipRow> createState() => _TipRowState();
}

class _TipRowState extends State<_TipRow> {
  late final controller = TextEditingController(text: widget.item.text);
  final focusNode = FocusNode();
  @override
  void initState() {
    super.initState();
    void selectionChanged() => Future.microtask(() {
      if (mounted && focusNode.hasFocus) {
        widget.onSelection?.call(controller.value);
      }
    });
    controller.addListener(selectionChanged);
    focusNode.addListener(selectionChanged);
  }

  @override
  void didUpdateWidget(covariant _TipRow oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (controller.text != widget.item.text) controller.text = widget.item.text;
  }

  @override
  void dispose() {
    controller.dispose();
    focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 6, 12, 14),
      decoration: BoxDecoration(
        color: p.surface.withValues(alpha: p.dark ? .3 : .58),
        borderRadius: BorderRadius.circular(18),
        border: Border.all(color: p.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  widget.label,
                  style: TextStyle(
                    color: p.accent,
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              ...widget.actions,
            ],
          ),
          TextField(
            key: widget.fieldKey,
            controller: controller,
            focusNode: focusNode,
            enabled: widget.enabled,
            minLines: 1,
            maxLines: 3,
            // Zero remaining characters still permits deletion and selection.
            maxLength: widget.maxLength > 0 ? widget.maxLength : null,
            inputFormatters: [
              if (widget.maxLength == 0)
                TextInputFormatter.withFunction(
                  (old, next) =>
                      next.text.characters.length <= old.text.characters.length
                      ? next
                      : old,
                ),
              FilteringTextInputFormatter.deny(
                RegExp(r'[\r\n]'),
                replacementString: ' ',
              ),
            ],
            style: TextStyle(color: p.ink, fontSize: 14, height: 1.65),
            decoration: InputDecoration(
              isDense: true,
              filled: false,
              counterText: '',
              hintText: Localizations.localeOf(context).languageCode == 'zh'
                  ? '写下一句话…'
                  : 'Write a little thought…',
              border: InputBorder.none,
              enabledBorder: InputBorder.none,
              focusedBorder: UnderlineInputBorder(
                borderSide: BorderSide(color: p.accent),
              ),
              contentPadding: const EdgeInsets.symmetric(vertical: 8),
            ),
            onChanged: widget.onChanged,
          ),
        ],
      ),
    );
  }
}
