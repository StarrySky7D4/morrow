import 'package:material_ui/material_ui.dart';
import '../appearance.dart';
import 'workbench_motion.dart';
import 'workbench_strings.dart';

/// Redesigned Masonry Grid card with colored accent pill, crisp typography,
/// task progress indicator, attachment pill, and hover-revealed quick actions.
class WorkbenchIdeaCard extends StatefulWidget {
  const WorkbenchIdeaCard({
    super.key,
    required this.palette,
    required this.id,
    required this.title,
    required this.description,
    required this.categoryLabel,
    required this.timeLabel,
    required this.icon,
    required this.color,
    required this.favorite,
    required this.favoriteTooltip,
    required this.attachmentCount,
    this.attachmentHintLabel,
    this.completedSteps = 0,
    this.totalSteps = 0,
    this.ambiguousSteps = 0,
    this.canEdit = true,
    required this.onTap,
    required this.onToggleFavorite,
    this.onEdit,
  });

  final Palette palette;
  final String id;
  final String title;
  final String description;
  final String categoryLabel;
  final String timeLabel;
  final IconData icon;
  final Color color;
  final bool favorite;
  final String favoriteTooltip;
  final int attachmentCount;
  final String? attachmentHintLabel;
  final int completedSteps;
  final int totalSteps;
  final int ambiguousSteps;
  final bool canEdit;
  final VoidCallback onTap;
  final VoidCallback onToggleFavorite;
  final VoidCallback? onEdit;

  @override
  State<WorkbenchIdeaCard> createState() => _WorkbenchIdeaCardState();
}

class _WorkbenchIdeaCardState extends State<WorkbenchIdeaCard> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final s = WorkbenchStrings.of(context);
    final accentTint = p.componentColor(widget.color);
    final readableTint = p.dark
        ? Color.lerp(accentTint, Colors.white, 0.34)!
        : Color.lerp(accentTint, Colors.black, 0.16)!;
    final progress = widget.totalSteps > 0
        ? (widget.completedSteps / widget.totalSteps).clamp(0.0, 1.0)
        : 0.0;

    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: Glass(
        componentId: 'card:${widget.id}',
        p: p,
        radius: 20,
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            onTap: widget.onTap,
            borderRadius: p.borderRadius(20),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(18, 15, 14, 15),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Container(
                        width: 3,
                        height: 20,
                        decoration: BoxDecoration(
                          color: readableTint,
                          borderRadius: BorderRadius.circular(2),
                        ),
                      ),
                      const SizedBox(width: 9),
                      Container(
                        width: 30,
                        height: 30,
                        decoration: BoxDecoration(
                          color: accentTint.withValues(alpha: 0.15),
                          borderRadius: p.borderRadius(10),
                        ),
                        child: Icon(widget.icon, color: readableTint, size: 17),
                      ),
                      const SizedBox(width: 8),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 7,
                          vertical: 3,
                        ),
                        decoration: BoxDecoration(
                          color: accentTint.withValues(alpha: 0.11),
                          borderRadius: p.borderRadius(6),
                        ),
                        child: Text(
                          widget.categoryLabel,
                          style: TextStyle(
                            fontSize: 9,
                            fontWeight: FontWeight.w600,
                            color: readableTint,
                          ),
                        ),
                      ),
                      const Spacer(),
                      IconButton(
                        tooltip: widget.favoriteTooltip,
                        constraints: const BoxConstraints(
                          minWidth: 30,
                          minHeight: 30,
                        ),
                        padding: EdgeInsets.zero,
                        onPressed: widget.onToggleFavorite,
                        icon: SoftSwap(
                          child: Icon(
                            widget.favorite
                                ? Icons.bookmark_rounded
                                : Icons.bookmark_border_rounded,
                            key: ValueKey(widget.favorite),
                            size: 17,
                            color: widget.favorite
                                ? p.accent
                                : p.muted.withValues(alpha: 0.55),
                          ),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  Text(
                    widget.title,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 15,
                      height: 1.35,
                      fontWeight: FontWeight.w600,
                      color: p.ink,
                    ),
                  ),
                  if (widget.description.trim().isNotEmpty) ...[
                    const SizedBox(height: 6),
                    Text(
                      widget.description,
                      maxLines: 3,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12,
                        color: p.muted,
                        height: 1.65,
                      ),
                    ),
                  ],
                  if (widget.totalSteps > 0) ...[
                    const SizedBox(height: 12),
                    Row(
                      children: [
                        Icon(
                          Icons.checklist_rounded,
                          size: 13,
                          color: readableTint,
                        ),
                        const SizedBox(width: 5),
                        Text(
                          '${widget.completedSteps}/${widget.totalSteps}',
                          style: TextStyle(
                            fontSize: 10,
                            fontWeight: FontWeight.w600,
                            color: p.ink,
                          ),
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: ClipRRect(
                            borderRadius: BorderRadius.circular(4),
                            child: TweenAnimationBuilder<double>(
                              tween: Tween(end: progress),
                              duration: workbenchDuration(
                                context,
                                WorkbenchDurations.medium,
                              ),
                              curve: WorkbenchCurves.springOut,
                              builder: (context, value, _) =>
                                  LinearProgressIndicator(
                                    value: value,
                                    minHeight: 4,
                                    backgroundColor: p.line,
                                    color: readableTint,
                                  ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                  if (widget.attachmentCount > 0 &&
                      widget.attachmentHintLabel != null) ...[
                    const SizedBox(height: 10),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 8,
                        vertical: 4,
                      ),
                      decoration: BoxDecoration(
                        color: p.accent.withValues(alpha: 0.08),
                        borderRadius: p.borderRadius(7),
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            Icons.attach_file_rounded,
                            size: 12,
                            color: p.accent,
                          ),
                          const SizedBox(width: 4),
                          Flexible(
                            child: Text(
                              widget.attachmentHintLabel!,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(fontSize: 10, color: p.accent),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                  const SizedBox(height: 14),
                  Row(
                    children: [
                      Text(
                        widget.timeLabel,
                        style: TextStyle(fontSize: 10, color: p.muted),
                      ),
                      const Spacer(),
                      IgnorePointer(
                        ignoring: !_hovered,
                        child: AnimatedOpacity(
                          opacity: _hovered ? 1 : 0,
                          alwaysIncludeSemantics: true,
                          duration: workbenchDuration(
                            context,
                            WorkbenchDurations.micro,
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              if (widget.canEdit && widget.onEdit != null)
                                Tooltip(
                                  message: s.editCard,
                                  child: InkWell(
                                    borderRadius: p.borderRadius(7),
                                    onTap: widget.onEdit,
                                    child: Padding(
                                      padding: const EdgeInsets.all(4),
                                      child: Icon(
                                        Icons.edit_outlined,
                                        size: 14,
                                        color: p.muted,
                                      ),
                                    ),
                                  ),
                                ),
                              const SizedBox(width: 2),
                              ExcludeSemantics(
                                child: Icon(
                                  Icons.arrow_forward_rounded,
                                  size: 14,
                                  color: p.accent,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Compact single-row horizontal card used when list view density is enabled.
class WorkbenchIdeaListRow extends StatelessWidget {
  const WorkbenchIdeaListRow({
    super.key,
    required this.palette,
    required this.id,
    required this.title,
    required this.description,
    required this.categoryLabel,
    required this.timeLabel,
    required this.icon,
    required this.color,
    required this.favorite,
    required this.favoriteTooltip,
    required this.attachmentCount,
    this.completedSteps = 0,
    this.totalSteps = 0,
    required this.onTap,
    required this.onToggleFavorite,
  });

  final Palette palette;
  final String id;
  final String title;
  final String description;
  final String categoryLabel;
  final String timeLabel;
  final IconData icon;
  final Color color;
  final bool favorite;
  final String favoriteTooltip;
  final int attachmentCount;
  final int completedSteps;
  final int totalSteps;
  final VoidCallback onTap;
  final VoidCallback onToggleFavorite;

  @override
  Widget build(BuildContext context) {
    final p = palette;
    final accentTint = p.componentColor(color);
    final readableTint = p.dark
        ? Color.lerp(accentTint, Colors.white, 0.34)!
        : Color.lerp(accentTint, Colors.black, 0.16)!;

    return Glass(
      componentId: 'card:$id',
      p: p,
      radius: 16,
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          borderRadius: p.borderRadius(16),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
            child: Row(
              children: [
                Container(
                  width: 32,
                  height: 32,
                  decoration: BoxDecoration(
                    color: accentTint.withValues(alpha: 0.15),
                    borderRadius: p.borderRadius(10),
                  ),
                  child: Icon(icon, color: readableTint, size: 17),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                          color: p.ink,
                        ),
                      ),
                      if (description.trim().isNotEmpty)
                        Text(
                          description.replaceAll('\n', ' '),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 11, color: p.muted),
                        ),
                    ],
                  ),
                ),
                const SizedBox(width: 10),
                if (totalSteps > 0)
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: Text(
                      '$completedSteps/$totalSteps',
                      style: TextStyle(
                        fontSize: 10,
                        fontWeight: FontWeight.w600,
                        color: readableTint,
                      ),
                    ),
                  ),
                if (attachmentCount > 0)
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.attach_file_rounded,
                          size: 13,
                          color: p.accent,
                        ),
                        const SizedBox(width: 2),
                        Text(
                          '$attachmentCount',
                          style: TextStyle(fontSize: 10, color: p.accent),
                        ),
                      ],
                    ),
                  ),
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 7,
                    vertical: 3,
                  ),
                  decoration: BoxDecoration(
                    color: accentTint.withValues(alpha: 0.10),
                    borderRadius: p.borderRadius(6),
                  ),
                  child: Text(
                    categoryLabel,
                    style: TextStyle(fontSize: 9, color: readableTint),
                  ),
                ),
                const SizedBox(width: 10),
                Text(
                  timeLabel,
                  style: TextStyle(fontSize: 10, color: p.muted),
                ),
                const SizedBox(width: 4),
                IconButton(
                  tooltip: favoriteTooltip,
                  constraints: const BoxConstraints(
                    minWidth: 30,
                    minHeight: 30,
                  ),
                  padding: EdgeInsets.zero,
                  onPressed: onToggleFavorite,
                  icon: SoftSwap(
                    child: Icon(
                      favorite
                          ? Icons.bookmark_rounded
                          : Icons.bookmark_border_rounded,
                      key: ValueKey(favorite),
                      size: 16,
                      color: favorite
                          ? p.accent
                          : p.muted.withValues(alpha: 0.55),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
