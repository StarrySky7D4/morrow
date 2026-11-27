import 'package:material_ui/material_ui.dart';

import '../appearance.dart';
import 'workbench_controls.dart';
import 'workbench_strings.dart';

@immutable
class FloatingWindowState {
  const FloatingWindowState({
    required this.id,
    required this.title,
    required this.icon,
    required this.offset,
    required this.size,
  });

  final String id;
  final String title;
  final IconData icon;
  final Offset offset;
  final Size size;

  static const double minWidth = 250;
  static const double maxWidth = 560;
  static const double minHeight = 190;
  static const double maxHeight = 620;

  FloatingWindowState copyWith({Offset? offset, Size? size}) =>
      FloatingWindowState(
        id: id,
        title: title,
        icon: icon,
        offset: offset ?? this.offset,
        size: size ?? this.size,
      );
}

class FloatingWidgetLayer extends StatelessWidget {
  const FloatingWidgetLayer({
    super.key,
    required this.palette,
    required this.windows,
    required this.viewportSize,
    required this.canDetach,
    required this.contentBuilder,
    required this.onMove,
    required this.onResize,
    required this.onFocus,
    required this.onDetach,
    this.onDragEnd,
    required this.onClose,
  });

  final Palette palette;
  final List<FloatingWindowState> windows;
  final Size viewportSize;
  final bool Function(String id) canDetach;
  final Widget Function(String id) contentBuilder;
  final void Function(String id, Offset delta) onMove;
  final void Function(String id, Offset delta) onResize;
  final ValueChanged<String> onFocus;
  final ValueChanged<String> onDetach;
  final void Function(String id, Size viewportSize)? onDragEnd;
  final ValueChanged<String> onClose;

  @override
  Widget build(BuildContext context) {
    if (windows.isEmpty) return const SizedBox.shrink();
    final p = palette;
    final s = WorkbenchStrings.of(context);

    return Stack(
      clipBehavior: Clip.none,
      children: [
        for (final win in windows)
          Positioned(
            left: canDetach(win.id)
                ? win.offset.dx.clamp(-180.0, viewportSize.width - 40.0)
                : win.offset.dx.clamp(
                    8.0,
                    (viewportSize.width - win.size.width - 8).clamp(8.0, 4000.0),
                  ),
            top: canDetach(win.id)
                ? win.offset.dy.clamp(-120.0, viewportSize.height - 40.0)
                : win.offset.dy.clamp(
                    8.0,
                    (viewportSize.height - win.size.height - 8).clamp(
                      8.0,
                      4000.0,
                    ),
                  ),
            width: win.size.width.clamp(
              FloatingWindowState.minWidth,
              FloatingWindowState.maxWidth,
            ),
            height: win.size.height.clamp(
              FloatingWindowState.minHeight,
              FloatingWindowState.maxHeight,
            ),
            child: GestureDetector(
              behavior: HitTestBehavior.translucent,
              onTapDown: (_) => onFocus(win.id),
              child: SizedBox.expand(
                key: ValueKey('floating-window-${win.id}'),
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    Positioned.fill(
                      child: Glass(
                        componentId: win.id,
                        p: p,
                        dialog: true,
                        radius: 22,
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            MouseRegion(
                              cursor: SystemMouseCursors.move,
                              child: GestureDetector(
                                key: ValueKey('floating-drag-${win.id}'),
                                behavior: HitTestBehavior.opaque,
                                onPanStart: (_) => onFocus(win.id),
                                onPanUpdate: (details) =>
                                    onMove(win.id, details.delta),
                                onPanEnd: (_) {
                                  if (canDetach(win.id)) {
                                    onDragEnd?.call(win.id, viewportSize);
                                  }
                                },
                                child: Padding(
                                  padding: const EdgeInsets.fromLTRB(
                                    14,
                                    10,
                                    8,
                                    8,
                                  ),
                                  child: Row(
                                    children: [
                                      Container(
                                        width: 26,
                                        height: 26,
                                        decoration: BoxDecoration(
                                          color: p.accent.withValues(
                                            alpha: 0.14,
                                          ),
                                          borderRadius: p.borderRadius(9),
                                        ),
                                        child: Icon(
                                          win.icon,
                                          size: 14,
                                          color: p.accent,
                                        ),
                                      ),
                                      const SizedBox(width: 9),
                                      Expanded(
                                        child: Column(
                                          mainAxisSize: MainAxisSize.min,
                                          crossAxisAlignment:
                                              CrossAxisAlignment.start,
                                          children: [
                                            Text(
                                              'MORROW / STUDIO',
                                              maxLines: 1,
                                              overflow: TextOverflow.ellipsis,
                                              style: TextStyle(
                                                fontSize: 9,
                                                letterSpacing: 1.0,
                                                color: p.muted,
                                              ),
                                            ),
                                            Text(
                                              win.title,
                                              maxLines: 1,
                                              overflow: TextOverflow.ellipsis,
                                              style: TextStyle(
                                                fontSize: 13,
                                                fontWeight: FontWeight.w600,
                                                color: p.ink,
                                              ),
                                            ),
                                          ],
                                        ),
                                      ),
                                      Icon(
                                        Icons.drag_indicator_rounded,
                                        size: 13,
                                        color: p.muted.withValues(alpha: 0.6),
                                      ),
                                      const SizedBox(width: 2),
                                      if (canDetach(win.id))
                                        WorkbenchMiniIconButton(
                                          key: ValueKey(
                                            'floating-detach-${win.id}',
                                          ),
                                          palette: p,
                                          icon: Icons.open_in_new_rounded,
                                          tooltip: s.widgetDetachTooltip,
                                          color: p.accent,
                                          onTap: () => onDetach(win.id),
                                        ),
                                      WorkbenchMiniIconButton(
                                        key: ValueKey(
                                          'floating-close-${win.id}',
                                        ),
                                        palette: p,
                                        icon: Icons.close_rounded,
                                        tooltip: s.widgetDockTooltip,
                                        color: p.muted,
                                        onTap: () => onClose(win.id),
                                      ),
                                    ],
                                  ),
                                ),
                              ),
                            ),
                            Divider(height: 1, color: p.line),
                            Expanded(
                              child: ScrollConfiguration(
                                behavior: ScrollConfiguration.of(
                                  context,
                                ).copyWith(scrollbars: false),
                                child: SingleChildScrollView(
                                  primary: false,
                                  child: contentBuilder(win.id),
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                    Positioned(
                      right: 2,
                      bottom: 2,
                      child: MouseRegion(
                        cursor: SystemMouseCursors.resizeUpLeftDownRight,
                        child: GestureDetector(
                          key: ValueKey('floating-resize-${win.id}'),
                          behavior: HitTestBehavior.opaque,
                          onPanUpdate: (details) =>
                              onResize(win.id, details.delta),
                          child: Padding(
                            padding: const EdgeInsets.all(6),
                            child: Opacity(
                              opacity: 0.72,
                              child: Icon(
                                Icons.south_east_rounded,
                                size: 13,
                                color: p.muted,
                              ),
                            ),
                          ),
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
  }
}
