import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class ComponentMenuAction {
  const ComponentMenuAction({
    required this.label,
    required this.icon,
    required this.onSelected,
    this.enabled = true,
  });
  final String label;
  final IconData icon;
  final VoidCallback onSelected;
  final bool enabled;
}

/// Secondary click and keyboard context menu share the existing command paths.
/// Primary taps remain with the child; opening a menu never triggers a command.
class ComponentContextMenu extends StatefulWidget {
  const ComponentContextMenu({
    super.key,
    required this.child,
    required this.actions,
  });
  final Widget child;
  final List<ComponentMenuAction> Function() actions;
  @override
  State<ComponentContextMenu> createState() => _ComponentContextMenuState();
}

class _ComponentContextMenuState extends State<ComponentContextMenu> {
  bool _open = false;
  Future<void> _show([Offset? position]) async {
    if (_open) return;
    final actions = widget.actions();
    if (actions.isEmpty) return;
    final overlay =
        Navigator.of(context).overlay?.context.findRenderObject() as RenderBox?;
    final box = context.findRenderObject() as RenderBox?;
    if (overlay == null || box == null || !box.hasSize) return;
    final point = overlay.globalToLocal(
      position ?? box.localToGlobal(box.size.center(Offset.zero)),
    );
    _open = true;
    try {
      final selected = await showMenu<int>(
        context: context,
        position: RelativeRect.fromRect(
          Rect.fromLTWH(point.dx, point.dy, 0, 0),
          Offset.zero & overlay.size,
        ),
        items: [
          for (var i = 0; i < actions.length; i++)
            PopupMenuItem<int>(
              value: i,
              enabled: actions[i].enabled,
              child: Row(
                children: [
                  Icon(actions[i].icon, size: 18),
                  const SizedBox(width: 12),
                  Flexible(child: Text(actions[i].label)),
                ],
              ),
            ),
        ],
      );
      if (mounted && selected != null && actions[selected].enabled) {
        actions[selected].onSelected();
      }
    } finally {
      _open = false;
    }
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.f10, shift: true): () => _show(),
      const SingleActivator(LogicalKeyboardKey.contextMenu): () => _show(),
    },
    child: Focus(
      canRequestFocus: false,
      child: GestureDetector(
        behavior: HitTestBehavior.deferToChild,
        onSecondaryTapUp: (details) => _show(details.globalPosition),
        child: widget.child,
      ),
    ),
  );
}
