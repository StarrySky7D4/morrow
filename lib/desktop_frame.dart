import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';
import 'appearance.dart';

bool get isWindowsDesktop =>
    !kIsWeb && defaultTargetPlatform == TargetPlatform.windows;

Future<void> initializeDesktopFrame() async {
  if (!isWindowsDesktop) return;
  await windowManager.ensureInitialized();
  await windowManager.setAsFrameless();
  await windowManager.setMinimumSize(const Size(390, 480));
}

/// The canvas extends behind the caption, including imported media and desktop
/// transparency. Keep resize hit areas separate from the visible border.
class DesktopFrame extends StatefulWidget {
  const DesktopFrame({super.key, required this.palette, required this.child});
  final Palette palette;
  final Widget child;
  @override
  State<DesktopFrame> createState() => _DesktopFrameState();
}

class _DesktopFrameState extends State<DesktopFrame> with WindowListener {
  static const shapeChannel = MethodChannel('morrow/window_shape');
  bool maximized = false;
  Future<void> applyShape() async {
    if (!isWindowsDesktop) return;
    try {
      await shapeChannel.invokeMethod<void>(
        'setRadius',
        widget.palette.windowRadius,
      );
    } on PlatformException catch (error) {
      FlutterError.reportError(
        FlutterErrorDetails(exception: error, library: 'morrow window shape'),
      );
    }
  }

  @override
  void initState() {
    super.initState();
    windowManager.addListener(this);
    applyShape();
    windowManager.isMaximized().then((value) {
      if (mounted) setState(() => maximized = value);
    });
  }

  @override
  void didUpdateWidget(DesktopFrame oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.palette.windowRadius != widget.palette.windowRadius) {
      applyShape();
    }
  }

  @override
  void dispose() {
    windowManager.removeListener(this);
    super.dispose();
  }

  @override
  void onWindowMaximize() => setState(() => maximized = true);
  @override
  void onWindowUnmaximize() => setState(() => maximized = false);

  @override
  Widget build(BuildContext context) {
    final p = widget.palette;
    final radius = BorderRadius.circular(maximized ? 0 : p.windowRadius);
    final contents = Stack(
      children: [
        widget.child,
        Positioned(
          top: 0,
          left: 0,
          right: 0,
          height: kWindowCaptionHeight,
          child: AnimatedContainer(
            key: const ValueKey('desktop-caption'),
            duration: motionDuration(context, 220),
            color: p.captionColor,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: DragToMoveArea(
                    child: Padding(
                      padding: const EdgeInsets.only(left: 16),
                      child: Row(
                        children: [
                          Icon(
                            Icons.all_inclusive_rounded,
                            size: 17,
                            color: p.accent,
                          ),
                          const SizedBox(width: 8),
                          Text(
                            'Morrow',
                            style: TextStyle(
                              inherit: false,
                              fontFamily: 'Segoe UI',
                              fontSize: 12,
                              color: p.ink,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
                Tooltip(
                  message: L10n.of(context).visualMinimize,
                  child: WindowCaptionButton.minimize(
                    brightness: p.dark ? Brightness.dark : Brightness.light,
                    onPressed: () async {
                      if (await windowManager.isMinimized()) {
                        await windowManager.restore();
                      } else {
                        await windowManager.minimize();
                      }
                    },
                  ),
                ),
                Tooltip(
                  message: maximized
                      ? L10n.of(context).visualRestoreWindow
                      : L10n.of(context).visualMaximize,
                  child: maximized
                      ? WindowCaptionButton.unmaximize(
                          brightness: p.dark
                              ? Brightness.dark
                              : Brightness.light,
                          onPressed: windowManager.unmaximize,
                        )
                      : WindowCaptionButton.maximize(
                          brightness: p.dark
                              ? Brightness.dark
                              : Brightness.light,
                          onPressed: windowManager.maximize,
                        ),
                ),
                Tooltip(
                  message: L10n.of(context).visualCloseWindow,
                  child: WindowCaptionButton.close(
                    brightness: p.dark ? Brightness.dark : Brightness.light,
                    onPressed: windowManager.close,
                  ),
                ),
              ],
            ),
          ),
        ),
        Positioned.fill(
          child: IgnorePointer(
            child: AnimatedContainer(
              key: const ValueKey('desktop-outline'),
              duration: motionDuration(context, 220),
              decoration: BoxDecoration(
                borderRadius: radius,
                border: Border.all(
                  color: maximized || p.backdrop == BackgroundMode.transparent
                      ? Colors.transparent
                      : p.glassEdge,
                ),
              ),
            ),
          ),
        ),
      ],
    );
    // The desktop frame is also used in MaterialApp.builder, outside the
    // Navigator's Overlay. Caption tooltips need their own full-window overlay.
    return _FrameOverlay(
      child: maximized
          ? contents
          : DragToResizeArea(
              resizeEdgeSize: 5,
              child: ClipRRect(
                key: const ValueKey('desktop-canvas-clip'),
                borderRadius: radius,
                // Composite overlapping canvas/caption/border layers first,
                // then apply coverage once to avoid dark seams at the arc.
                clipBehavior: Clip.antiAliasWithSaveLayer,
                child: contents,
              ),
            ),
    );
  }
}

class _FrameOverlay extends StatefulWidget {
  const _FrameOverlay({required this.child});
  final Widget child;
  @override
  State<_FrameOverlay> createState() => _FrameOverlayState();
}

class _FrameOverlayState extends State<_FrameOverlay> {
  late final OverlayEntry _content = OverlayEntry(builder: (_) => widget.child);

  @override
  void didUpdateWidget(_FrameOverlay oldWidget) {
    super.didUpdateWidget(oldWidget);
    _content.markNeedsBuild();
  }

  @override
  void dispose() {
    _content.remove();
    _content.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Overlay(initialEntries: [_content]);
}
