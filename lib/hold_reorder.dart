import 'dart:async';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'appearance.dart';

/// Move relative to an identity, never to an index captured before the drag.
List<T> moveRelative<T>(List<T> values, T source, T target, bool after) {
  final result = [...values];
  if (source == target ||
      !result.contains(source) ||
      !result.contains(target)) {
    return result;
  }
  result.remove(source);
  result.insert(result.indexOf(target) + (after ? 1 : 0), source);
  return result;
}

class _HeldItem {
  _HeldItem(this.scope, this.id, this.revision);
  final Object scope, id, revision;
  final scroll = _DragScroll();
}

/// Keeps edge scrolling alive if a virtualized source card leaves the viewport.
/// The pointer route is removed on release/cancel, independent of widget lifetime.
class _DragScroll {
  int? pointer;
  Offset? position;
  ScrollableState? viewport;
  Timer? timer;
  void event(PointerEvent event) {
    if (event.pointer != pointer) return;
    position = event.position;
    if (event is PointerUpEvent || event is PointerCancelEvent) stop();
  }

  void start(BuildContext context) {
    viewport = Scrollable.maybeOf(context);
    if (viewport == null || pointer == null) return;
    GestureBinding.instance.pointerRouter.addGlobalRoute(event);
    timer = Timer.periodic(const Duration(milliseconds: 16), (_) {
      final view = viewport;
      if (view == null || !view.mounted) {
        stop();
        return;
      }
      final render = view.context.findRenderObject();
      if (render is! RenderBox || !render.hasSize || position == null) return;
      final point = render.globalToLocal(position!);
      if (point.dx < 0 ||
          point.dx > render.size.width ||
          point.dy < -32 ||
          point.dy > render.size.height + 32) {
        return;
      }
      final delta = point.dy < 64
          ? -12.0
          : point.dy > render.size.height - 64
          ? 12.0
          : 0.0;
      final scroll = view.position;
      if (!scroll.hasContentDimensions || delta == 0) return;
      final next = (scroll.pixels + delta).clamp(
        scroll.minScrollExtent,
        scroll.maxScrollExtent,
      );
      if (next != scroll.pixels) scroll.jumpTo(next);
    });
  }

  void stop() {
    if (timer == null) return;
    timer!.cancel();
    timer = null;
    GestureBinding.instance.pointerRouter.removeGlobalRoute(event);
    viewport = null;
  }
}

/// Shared long-press interaction for both masonry cards and list rows. Editors
/// use a handle so holding editable text still invokes selection normally.
class HoldReorder extends StatefulWidget {
  const HoldReorder({
    super.key,
    required this.scope,
    required this.id,
    required this.revision,
    required this.label,
    required this.enabled,
    required this.onMove,
    this.child,
    this.builder,
  }) : assert((child == null) != (builder == null));
  final Object scope, id, revision;
  final String label;
  final bool enabled;
  final void Function(Object source, Object target, bool after) onMove;
  final Widget? child;
  final Widget Function(Widget handle)? builder;
  @override
  State<HoldReorder> createState() => _HoldReorderState();
}

class _HoldReorderState extends State<HoldReorder> {
  bool? after;
  _HeldItem? _payload;
  bool accepts(_HeldItem item) =>
      widget.enabled &&
      identical(item.scope, widget.scope) &&
      item.revision == widget.revision &&
      item.id != widget.id;
  bool trailing(Offset point) {
    final box = context.findRenderObject()! as RenderBox;
    return box.globalToLocal(point).dy >= box.size.height / 2;
  }

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    if (_payload?.scope != widget.scope ||
        _payload?.id != widget.id ||
        _payload?.revision != widget.revision) {
      _payload = _HeldItem(widget.scope, widget.id, widget.revision);
    }
    final payload = _payload!;
    final zh = Localizations.localeOf(context).languageCode == 'zh';
    Widget draggable(Widget child) => Listener(
      onPointerDown: (e) {
        if (payload.scroll.timer != null) return;
        payload.scroll.pointer = e.pointer;
        payload.scroll.position = e.position;
      },
      child: LongPressDraggable<_HeldItem>(
        data: payload,
        delay: const Duration(milliseconds: 380),
        maxSimultaneousDrags: widget.enabled ? 1 : 0,
        allowedButtonsFilter: (buttons) => buttons == kPrimaryMouseButton,
        dragAnchorStrategy: pointerDragAnchorStrategy,
        onDragStarted: () => payload.scroll.start(context),
        onDragCompleted: payload.scroll.stop,
        onDraggableCanceled: (_, _) => payload.scroll.stop(),
        feedback: Material(
          color: Colors.transparent,
          child: Container(
            constraints: const BoxConstraints(maxWidth: 240),
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
            decoration: BoxDecoration(
              color: p.surface,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: p.accent),
              boxShadow: const [
                BoxShadow(blurRadius: 16, color: Colors.black26),
              ],
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.drag_indicator_rounded, color: p.accent, size: 18),
                const SizedBox(width: 8),
                Flexible(
                  child: Text(
                    widget.label,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(color: p.ink, fontSize: 13),
                  ),
                ),
              ],
            ),
          ),
        ),
        child: child,
      ),
    );
    final handle = draggable(
      Tooltip(
        message: zh ? '长按拖拽排序' : 'Hold to reorder',
        child: SizedBox(
          key: ValueKey('drag-handle:${widget.id}'),
          width: 36,
          height: 40,
          child: Icon(
            Icons.drag_indicator_rounded,
            size: 19,
            color: widget.enabled ? p.muted : p.muted.withValues(alpha: .3),
          ),
        ),
      ),
    );
    return DragTarget<_HeldItem>(
      onWillAcceptWithDetails: (d) => accepts(d.data),
      onMove: (d) {
        if (accepts(d.data)) setState(() => after = trailing(d.offset));
      },
      onLeave: (_) {
        if (mounted) setState(() => after = null);
      },
      onAcceptWithDetails: (d) {
        final valid = accepts(d.data);
        setState(() => after = null);
        if (valid) widget.onMove(d.data.id, widget.id, trailing(d.offset));
      },
      builder: (_, candidates, _) => DecoratedBox(
        decoration: BoxDecoration(
          border: candidates.isEmpty
              ? null
              : Border(
                  top: after == false
                      ? BorderSide(color: p.accent, width: 3)
                      : BorderSide.none,
                  bottom: after != false
                      ? BorderSide(color: p.accent, width: 3)
                      : BorderSide.none,
                ),
        ),
        position: DecorationPosition.foreground,
        child: widget.builder?.call(handle) ?? draggable(widget.child!),
      ),
    );
  }
}
