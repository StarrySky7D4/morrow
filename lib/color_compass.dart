import 'package:morrow_i18n/morrow_i18n.dart';
import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'appearance.dart';
import 'animated_slider_style.dart';

class ColorCompassDialog extends StatefulWidget {
  const ColorCompassDialog({
    super.key,
    required this.initial,
    this.title,
    this.onChanged,
    this.canEdit,
  });
  final Color initial;
  final String? title;
  final ValueChanged<Color>? onChanged;

  /// Rechecked against the live palette when theme ownership changes.
  final bool Function(Palette)? canEdit;
  @override
  State<ColorCompassDialog> createState() => _ColorCompassDialogState();
}

class _ColorCompassDialogState extends State<ColorCompassDialog> {
  late HSVColor hsv;
  final hex = TextEditingController();
  bool invalid = false;
  bool revoked = false;
  bool closing = false;
  bool get editable =>
      mounted &&
      !closing &&
      !revoked &&
      (widget.canEdit?.call(AppearanceScope.of(context)) ?? true);
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // An interrupted edit must never become committable again later.
    if (!(widget.canEdit?.call(AppearanceScope.of(context)) ?? true)) {
      revoked = true;
    }
  }

  @override
  void initState() {
    super.initState();
    hsv = HSVColor.fromColor(widget.initial);
    syncHex();
  }

  @override
  void dispose() {
    hex.dispose();
    super.dispose();
  }

  void syncHex() => hex.text =
      '#${hsv.toColor().toARGB32().toRadixString(16).padLeft(8, '0').substring(2).toUpperCase()}';
  void setColor(HSVColor value) {
    if (!editable) return;
    setState(() {
      hsv = value;
      invalid = false;
      syncHex();
      widget.onChanged?.call(hsv.toColor());
    });
  }

  void wheel(Offset point, double size) {
    final delta = point - Offset(size / 2, size / 2);
    setColor(
      hsv
          .withHue((math.atan2(delta.dy, delta.dx) * 180 / math.pi + 360) % 360)
          .withSaturation((delta.distance / (size / 2 - 10)).clamp(0, 1)),
    );
  }

  Color? readHex() {
    if (!editable) return null;
    final value = hex.text.trim().replaceFirst('#', '');
    if (!RegExp(r'^[0-9a-fA-F]{6}$').hasMatch(value)) {
      setState(() => invalid = true);
      return null;
    }
    return Color(0xFF000000 | int.parse(value, radix: 16));
  }

  bool applyHex() {
    final color = readHex();
    if (color == null) return false;
    setColor(HSVColor.fromColor(color));
    return true;
  }

  void submit() {
    final color = readHex();
    if (color == null) return;
    // Commit once after the route closes. Do not rewrite the focused input or
    // publish another live preview in the same frame that removes its route.
    closing = true;
    Navigator.pop(context, color);
  }

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      child: StudioDialog(
        title: widget.title ?? L10n.of(context).visualColorTitle,
        subtitle: !revoked
            ? L10n.of(context).visualColorGuide
            : Localizations.localeOf(context).languageCode == 'zh'
            ? '主题已接管配色，此次编辑已停用。关闭主题后请重新打开调色罗盘。'
            : 'The theme controls this color. Close this picker and reopen it after disabling the theme.',
        icon: Icons.palette_outlined,
        content: IgnorePointer(
          ignoring: !editable,
          child: ExcludeFocus(
            excluding: !editable,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                LayoutBuilder(
                  builder: (_, constraints) {
                    final size = math.min(232.0, constraints.maxWidth);
                    return Center(
                      child: GestureDetector(
                        key: const ValueKey('color-wheel'),
                        onPanDown: (details) =>
                            wheel(details.localPosition, size),
                        onPanUpdate: (details) =>
                            wheel(details.localPosition, size),
                        child: SizedBox(
                          width: size,
                          height: size,
                          child: CustomPaint(painter: ColorWheelPainter(hsv)),
                        ),
                      ),
                    );
                  },
                ),
                const SizedBox(height: 14),
                Row(
                  children: [
                    Icon(Icons.brightness_6_outlined, size: 17, color: p.muted),
                    Expanded(
                      child: AnimatedSliderStyle(
                        palette: p,
                        child: Slider(
                          key: const ValueKey('color-brightness'),
                          value: hsv.value,
                          onChanged: (value) => setColor(hsv.withValue(value)),
                          label: '${(hsv.value * 100).round()}%',
                        ),
                      ),
                    ),
                    Text(
                      '${(hsv.value * 100).round()}%',
                      style: TextStyle(color: p.muted, fontSize: 11),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    AnimatedContainer(
                      duration: motionDuration(context, 150),
                      width: 54,
                      height: 54,
                      decoration: BoxDecoration(
                        color: hsv.toColor(),
                        borderRadius: BorderRadius.circular(16),
                        border: Border.all(color: p.line),
                      ),
                    ),
                    const SizedBox(width: 14),
                    Expanded(
                      child: TextField(
                        enabled: editable,
                        key: const ValueKey('color-hex'),
                        controller: hex,
                        onSubmitted: (_) => applyHex(),
                        onChanged: (_) {
                          if (invalid) setState(() => invalid = false);
                        },
                        decoration: InputDecoration(
                          labelText: L10n.of(context).visualHexColor,
                          errorText: invalid
                              ? L10n.of(context).visualHexInvalid
                              : null,
                          hintText: '#8E7CC3',
                          suffixIcon: Semantics(
                            container: true,
                            explicitChildNodes: true,
                            child: IconButton(
                              tooltip: L10n.of(context).visualPreviewColor,
                              onPressed: applyHex,
                              icon: const Icon(Icons.check, size: 18),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(L10n.of(context).visualCancel),
          ),
          FilledButton(
            onPressed: editable ? submit : null,
            child: Text(L10n.of(context).visualApplyColor),
          ),
        ],
      ),
    );
  }
}

class ColorWheelPainter extends CustomPainter {
  ColorWheelPainter(this.hsv);
  final HSVColor hsv;
  @override
  void paint(Canvas canvas, Size size) {
    final center = size.center(Offset.zero), radius = size.width / 2 - 10;
    final rect = Rect.fromCircle(center: center, radius: radius);
    canvas.drawCircle(
      center,
      radius,
      Paint()
        ..shader = const SweepGradient(
          colors: [
            Color(0xFFFF0000),
            Color(0xFFFFFF00),
            Color(0xFF00FF00),
            Color(0xFF00FFFF),
            Color(0xFF0000FF),
            Color(0xFFFF00FF),
            Color(0xFFFF0000),
          ],
        ).createShader(rect),
    );
    canvas.drawCircle(
      center,
      radius,
      Paint()
        ..shader = const RadialGradient(
          colors: [Colors.white, Color(0x00FFFFFF)],
        ).createShader(rect),
    );
    canvas.drawCircle(
      center,
      radius,
      Paint()..color = Colors.black.withValues(alpha: 1 - hsv.value),
    );
    final angle = hsv.hue * math.pi / 180;
    final pointer =
        center +
        Offset(math.cos(angle), math.sin(angle)) * radius * hsv.saturation;
    canvas.drawCircle(
      pointer,
      9,
      Paint()
        ..color = Colors.black.withValues(alpha: .22)
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3),
    );
    canvas.drawCircle(pointer, 8, Paint()..color = hsv.toColor());
    canvas.drawCircle(
      pointer,
      8,
      Paint()
        ..color = Colors.white
        ..style = PaintingStyle.stroke
        ..strokeWidth = 3,
    );
  }

  @override
  bool shouldRepaint(ColorWheelPainter old) => hsv != old.hsv;
}
