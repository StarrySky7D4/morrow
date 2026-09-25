import 'package:flutter/material.dart';
import 'tip_list_editor.dart';
import 'tip_preferences.dart';

/// Keeps the existing draft/paste protocol's newline field while presenting
/// individually editable rows. Row IDs are view-local, never business TaskIds.
class CardTipsEditor extends StatefulWidget {
  const CardTipsEditor({
    super.key,
    required this.controller,
    required this.focusNode,
    required this.enabled,
    required this.title,
  });
  final TextEditingController controller;
  final FocusNode focusNode;
  final bool enabled;
  final String title;
  @override
  State<CardTipsEditor> createState() => _CardTipsEditorState();
}

class _CardTipsEditorState extends State<CardTipsEditor> {
  List<TipItem> items = [];
  bool writing = false;
  @override
  void initState() {
    super.initState();
    read();
    widget.controller.addListener(changed);
  }

  void read() {
    final lines = widget.controller.text.isEmpty
        ? <String>[]
        : widget.controller.text.split('\n');
    items = [
      for (var i = 0; i < lines.length; i++)
        TipItem(i < items.length ? items[i].id : TipItem.empty().id, lines[i]),
    ];
  }

  void changed() {
    if (writing ||
        items.map((v) => v.text).join('\n') == widget.controller.text) {
      return;
    }
    setState(read);
  }

  @override
  void didUpdateWidget(CardTipsEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(changed);
      read();
      widget.controller.addListener(changed);
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(changed);
    super.dispose();
  }

  void write(List<TipItem>? next, String? _) {
    items = next ?? [];
    writing = true;
    widget.controller.value = TextEditingValue(
      text: items.map((v) => v.text).join('\n'),
    );
    writing = false;
  }

  void selection(String id, TextEditingValue value) {
    final index = items.indexWhere((v) => v.id == id);
    if (index < 0 || !value.selection.isValid) return;
    final offset = items
        .take(index)
        .fold<int>(0, (n, v) => n + v.text.length + 1);
    final selection = TextSelection(
      baseOffset: offset + value.selection.baseOffset,
      extentOffset: offset + value.selection.extentOffset,
    );
    if (selection.end > widget.controller.text.length) return;
    writing = true;
    widget.controller.selection = selection;
    writing = false;
  }

  @override
  Widget build(BuildContext context) => Focus(
    focusNode: widget.focusNode,
    child: TipListEditor(
      initial: items,
      defaults: const [],
      daily: false,
      enabled: widget.enabled,
      title: widget.title,
      description: Localizations.localeOf(context).languageCode == 'zh'
          ? '一条一张卡片，可添加、删除或拖拽排序。随当前卡片一起保存。'
          : 'Add, edit or reorder individual tips. Saved with this card.',
      allowReset: false,
      syncInitial: true,
      maxTextLength: 1000,
      onSelection: selection,
      onChanged: write,
    ),
  );
}
