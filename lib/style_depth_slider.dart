import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'appearance.dart';
import 'animated_slider_style.dart';

class StyleDepthSlider extends StatelessWidget {
  const StyleDepthSlider({
    super.key,
    required this.palette,
    required this.value,
    required this.onChanged,
    this.onChangeEnd,
    this.onReset,
  });
  final Palette palette;
  final double value;
  final ValueChanged<double>? onChanged, onChangeEnd;
  final VoidCallback? onReset;
  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                l.mainStyleDepth,
                style: TextStyle(color: palette.ink),
              ),
            ),
            Text(
              '${(value * 100).round()}%',
              style: TextStyle(color: palette.accent),
            ),
          ],
        ),
        AnimatedSliderStyle(
          palette: palette,
          child: Slider(
            value: value,
            min: 0,
            max: 2,
            divisions: 100,
            label: '${(value * 100).round()}%',
            semanticFormatterCallback: (v) =>
                '${l.mainStyleDepth}: ${(v * 100).round()}%',
            onChanged: onChanged,
            onChangeEnd: onChangeEnd,
          ),
        ),
        Text(
          l.mainStyleDepthGuide,
          style: TextStyle(color: palette.muted, fontSize: 12, height: 1.4),
        ),
        if (onReset != null)
          Align(
            alignment: Alignment.centerRight,
            child: TextButton(
              onPressed: onReset,
              child: Text(l.mainStyleDepthReset),
            ),
          ),
      ],
    );
  }
}
