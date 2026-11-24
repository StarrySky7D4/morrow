import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import '../appearance.dart';
import '../plugins/workbench_ids.dart';
import '../plugins/workbench_labels.dart';
import 'workbench_motion.dart';
import 'workbench_rail.dart';
import 'workbench_strings.dart';

class CommandPaletteCardItem {
  const CommandPaletteCardItem({
    required this.id,
    required this.title,
    required this.subtitle,
    required this.category,
    required this.icon,
    required this.color,
    required this.onOpen,
  });

  final String id;
  final String title;
  final String subtitle;
  final String category;
  final IconData icon;
  final Color color;
  final VoidCallback onOpen;
}

class CommandPaletteActionItem {
  const CommandPaletteActionItem({
    required this.id,
    required this.label,
    required this.icon,
    required this.onRun,
    this.shortcut,
  });

  final String id;
  final String label;
  final IconData icon;
  final VoidCallback onRun;
  final String? shortcut;
}

Future<void> showCommandPalette({
  required BuildContext context,
  required Palette palette,
  required List<CommandPaletteCardItem> cards,
  required List<CommandPaletteActionItem> actions,
  required Map<WorkbenchPage, int> pageCounts,
  required ValueChanged<WorkbenchPage> onSelectPage,
  required ValueChanged<String> onApplyWorkspaceFilter,
}) => showStudioDialog<void>(
  context: context,
  builder: (_) => _CommandPaletteDialog(
    palette: palette,
    cards: cards,
    actions: actions,
    pageCounts: pageCounts,
    onSelectPage: onSelectPage,
    onApplyWorkspaceFilter: onApplyWorkspaceFilter,
  ),
);

class _PaletteEntry {
  const _PaletteEntry({
    required this.keyId,
    required this.section,
    required this.title,
    required this.icon,
    required this.onSelect,
    this.subtitle,
    this.badge,
    this.tint,
  });

  final String keyId;
  final String section;
  final String title;
  final String? subtitle;
  final String? badge;
  final IconData icon;
  final Color? tint;
  final VoidCallback onSelect;
}

class _CommandPaletteDialog extends StatefulWidget {
  const _CommandPaletteDialog({
    required this.palette,
    required this.cards,
    required this.actions,
    required this.pageCounts,
    required this.onSelectPage,
    required this.onApplyWorkspaceFilter,
  });

  final Palette palette;
  final List<CommandPaletteCardItem> cards;
  final List<CommandPaletteActionItem> actions;
  final Map<WorkbenchPage, int> pageCounts;
  final ValueChanged<WorkbenchPage> onSelectPage;
  final ValueChanged<String> onApplyWorkspaceFilter;

  @override
  State<_CommandPaletteDialog> createState() => _CommandPaletteDialogState();
}

class _CommandPaletteDialogState extends State<_CommandPaletteDialog> {
  final _queryController = TextEditingController();
  final _scrollController = ScrollController();
  int _activeIndex = 0;

  @override
  void dispose() {
    _queryController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  List<_PaletteEntry> _buildEntries(BuildContext context) {
    final s = WorkbenchStrings.of(context);
    final labels = WorkbenchLabelsScope.of(context);
    final q = _queryController.text.trim().toLowerCase();
    final entries = <_PaletteEntry>[];

    if (q.isNotEmpty) {
      entries.add(
        _PaletteEntry(
          keyId: 'palette-apply-filter',
          section: s.commandSectionActions,
          title: s.commandFilterWorkspace(_queryController.text.trim()),
          icon: Icons.filter_alt_outlined,
          onSelect: () =>
              widget.onApplyWorkspaceFilter(_queryController.text.trim()),
        ),
      );
    }

    final matchingCards = widget.cards
        .where(
          (c) =>
              q.isEmpty ||
              c.title.toLowerCase().contains(q) ||
              c.subtitle.toLowerCase().contains(q) ||
              c.category.toLowerCase().contains(q),
        )
        .take(q.isEmpty ? 4 : 8);
    for (final card in matchingCards) {
      entries.add(
        _PaletteEntry(
          keyId: 'palette-card-${card.id}',
          section: s.commandSectionCards,
          title: card.title,
          subtitle: card.subtitle,
          badge: card.category,
          icon: card.icon,
          tint: card.color,
          onSelect: card.onOpen,
        ),
      );
    }

    for (final page in WorkbenchPage.values) {
      final pageTitle = labels.page(page);
      if (q.isEmpty ||
          pageTitle.toLowerCase().contains(q) ||
          page.name.toLowerCase().contains(q)) {
        final count = widget.pageCounts[page];
        entries.add(
          _PaletteEntry(
            keyId: 'palette-page-${page.id}',
            section: s.commandSectionNavigate,
            title: pageTitle,
            badge: count == null ? null : '$count',
            icon: WorkbenchRail.iconForPage(page, active: true),
            onSelect: () => widget.onSelectPage(page),
          ),
        );
      }
    }

    for (final action in widget.actions) {
      if (q.isEmpty || action.label.toLowerCase().contains(q)) {
        entries.add(
          _PaletteEntry(
            keyId: 'palette-action-${action.id}',
            section: s.commandSectionActions,
            title: action.label,
            badge: action.shortcut,
            icon: action.icon,
            onSelect: action.onRun,
          ),
        );
      }
    }

    return entries;
  }

  void _runEntry(_PaletteEntry entry) {
    Navigator.of(context).pop();
    entry.onSelect();
  }

  void _moveSelection(int delta, int count) {
    if (count == 0) return;
    setState(() {
      _activeIndex = (_activeIndex + delta).clamp(0, count - 1);
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scrollController.hasClients) return;
      final position = _scrollController.position;
      const estimatedExtent = 54.0;
      final targetTop = _activeIndex * estimatedExtent;
      final targetBottom = targetTop + estimatedExtent;
      final viewTop = position.pixels;
      final viewBottom = viewTop + position.viewportDimension;
      double? destination;
      if (targetTop < viewTop) {
        destination = targetTop;
      } else if (targetBottom > viewBottom) {
        destination = targetBottom - position.viewportDimension;
      }
      if (destination != null) {
        _scrollController.animateTo(
          destination.clamp(0.0, position.maxScrollExtent),
          duration: workbenchDuration(context, WorkbenchDurations.micro),
          curve: Curves.easeOutCubic,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final s = WorkbenchStrings.of(context);
    final entries = _buildEntries(context);
    final safeIndex = entries.isEmpty
        ? 0
        : _activeIndex.clamp(0, entries.length - 1);

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): () =>
            Navigator.of(context).pop(),
        const SingleActivator(LogicalKeyboardKey.arrowDown): () =>
            _moveSelection(1, entries.length),
        const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
            _moveSelection(-1, entries.length),
        const SingleActivator(LogicalKeyboardKey.enter): () {
          if (entries.isNotEmpty) _runEntry(entries[safeIndex]);
        },
      },
      child: Center(
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 560, maxHeight: 500),
            child: Material(
              color: Colors.transparent,
              child: Glass(
                key: const ValueKey('command-palette-dialog'),
                p: p,
                dialog: true,
                radius: 22,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Padding(
                      padding: const EdgeInsets.fromLTRB(16, 14, 14, 12),
                      child: Row(
                        children: [
                          Icon(Icons.bolt_rounded, size: 19, color: p.accent),
                          const SizedBox(width: 10),
                          Expanded(
                            child: TextField(
                              key: const ValueKey('command-palette-input'),
                              controller: _queryController,
                              autofocus: true,
                              onChanged: (_) => setState(() => _activeIndex = 0),
                              onSubmitted: (_) {
                                if (entries.isNotEmpty) {
                                  _runEntry(entries[safeIndex]);
                                }
                              },
                              style: TextStyle(fontSize: 13, color: p.ink),
                              decoration: InputDecoration(
                                isDense: true,
                                filled: false,
                                hintText: s.commandHint,
                                hintStyle: TextStyle(
                                  fontSize: 13,
                                  color: p.muted,
                                ),
                                border: InputBorder.none,
                                enabledBorder: InputBorder.none,
                                focusedBorder: InputBorder.none,
                                contentPadding: EdgeInsets.zero,
                              ),
                            ),
                          ),
                          if (_queryController.text.isNotEmpty)
                            IconButton(
                              visualDensity: VisualDensity.compact,
                              onPressed: () => setState(() {
                                _queryController.clear();
                                _activeIndex = 0;
                              }),
                              icon: Icon(
                                Icons.close_rounded,
                                size: 16,
                                color: p.muted,
                              ),
                            ),
                          Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 7,
                              vertical: 3,
                            ),
                            decoration: BoxDecoration(
                              color: p.ink.withValues(alpha: 0.06),
                              borderRadius: p.borderRadius(7),
                            ),
                            child: Text(
                              'Esc',
                              style: TextStyle(fontSize: 10, color: p.muted),
                            ),
                          ),
                        ],
                      ),
                    ),
                    Divider(height: 1, color: p.line),
                    Flexible(
                      child: entries.isEmpty
                          ? Padding(
                              padding: const EdgeInsets.all(32),
                              child: Column(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(
                                    Icons.search_off_rounded,
                                    color: p.muted,
                                    size: 26,
                                  ),
                                  const SizedBox(height: 10),
                                  Text(
                                    s.commandEmpty,
                                    style: TextStyle(
                                      color: p.muted,
                                      fontSize: 12,
                                    ),
                                  ),
                                ],
                              ),
                            )
                          : ListView.builder(
                              controller: _scrollController,
                              shrinkWrap: true,
                              padding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 10,
                              ),
                              itemCount: entries.length,
                              itemBuilder: (context, index) {
                                final entry = entries[index];
                                final showSection =
                                    index == 0 ||
                                    entries[index - 1].section != entry.section;
                                final selected = index == safeIndex;
                                return Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    if (showSection)
                                      Padding(
                                        padding: EdgeInsets.fromLTRB(
                                          10,
                                          index == 0 ? 2 : 10,
                                          10,
                                          6,
                                        ),
                                        child: Text(
                                          entry.section,
                                          style: TextStyle(
                                            fontSize: 10,
                                            fontWeight: FontWeight.w600,
                                            letterSpacing: 0.8,
                                            color: p.muted,
                                          ),
                                        ),
                                      ),
                                    AnimatedContainer(
                                      duration: workbenchDuration(
                                        context,
                                        WorkbenchDurations.micro,
                                      ),
                                      decoration: BoxDecoration(
                                        color: selected
                                            ? p.accent.withValues(
                                                alpha: p.dark ? 0.2 : 0.12,
                                              )
                                            : Colors.transparent,
                                        borderRadius: p.borderRadius(12),
                                        border: Border.all(
                                          color: selected
                                              ? p.accent.withValues(alpha: 0.35)
                                              : Colors.transparent,
                                        ),
                                      ),
                                      child: InkWell(
                                        key: ValueKey(entry.keyId),
                                        borderRadius: p.borderRadius(12),
                                        onHover: (hovered) {
                                          if (hovered && _activeIndex != index) {
                                            setState(() => _activeIndex = index);
                                          }
                                        },
                                        onTap: () => _runEntry(entry),
                                        child: Padding(
                                          padding: const EdgeInsets.symmetric(
                                            horizontal: 10,
                                            vertical: 9,
                                          ),
                                          child: Row(
                                            children: [
                                              Container(
                                                width: 30,
                                                height: 30,
                                                decoration: BoxDecoration(
                                                  color: (entry.tint ?? p.accent)
                                                      .withValues(alpha: 0.14),
                                                  borderRadius: p.borderRadius(
                                                    9,
                                                  ),
                                                ),
                                                child: Icon(
                                                  entry.icon,
                                                  size: 16,
                                                  color: entry.tint ?? p.accent,
                                                ),
                                              ),
                                              const SizedBox(width: 10),
                                              Expanded(
                                                child: Column(
                                                  crossAxisAlignment:
                                                      CrossAxisAlignment.start,
                                                  children: [
                                                    Text(
                                                      entry.title,
                                                      maxLines: 1,
                                                      overflow:
                                                          TextOverflow.ellipsis,
                                                      style: TextStyle(
                                                        fontSize: 12,
                                                        fontWeight: selected
                                                            ? FontWeight.w600
                                                            : FontWeight.w500,
                                                        color: p.ink,
                                                      ),
                                                    ),
                                                    if (entry.subtitle != null &&
                                                        entry.subtitle!
                                                            .isNotEmpty)
                                                      Text(
                                                        entry.subtitle!,
                                                        maxLines: 1,
                                                        overflow: TextOverflow
                                                            .ellipsis,
                                                        style: TextStyle(
                                                          fontSize: 10,
                                                          color: p.muted,
                                                        ),
                                                      ),
                                                  ],
                                                ),
                                              ),
                                              if (entry.badge != null)
                                                Container(
                                                  margin: const EdgeInsets.only(
                                                    left: 8,
                                                  ),
                                                  padding:
                                                      const EdgeInsets.symmetric(
                                                        horizontal: 7,
                                                        vertical: 3,
                                                      ),
                                                  decoration: BoxDecoration(
                                                    color: p.ink.withValues(
                                                      alpha: 0.06,
                                                    ),
                                                    borderRadius: p.borderRadius(
                                                      7,
                                                    ),
                                                  ),
                                                  child: Text(
                                                    entry.badge!,
                                                    style: TextStyle(
                                                      fontSize: 10,
                                                      color: p.muted,
                                                    ),
                                                  ),
                                                ),
                                            ],
                                          ),
                                        ),
                                      ),
                                    ),
                                  ],
                                );
                              },
                            ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
