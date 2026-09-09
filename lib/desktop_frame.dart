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
            child: WindowCaption(
              backgroundColor: Colors.transparent,
              brightness: p.dark ? Brightness.dark : Brightness.light,
              title: Row(
                children: [
                  Icon(Icons.all_inclusive_rounded, size: 17, color: p.accent),
                  const SizedBox(width: 8),
                  Text('Morrow', style: TextStyle(fontSize: 12, color: p.ink)),
                ],
              ),
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
    return maximized
        ? contents
        : DragToResizeArea(
            resizeEdgeSize: 5,
            child: ClipRRect(borderRadius: radius, child: contents),
          );
  }
}
