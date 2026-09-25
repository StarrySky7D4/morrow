import 'package:flutter/material.dart';

/// A presentation-only overlay. The original appearance preferences stay intact.
@immutable
class UiThemeTokens {
  const UiThemeTokens({
    required this.background,
    required this.surface,
    required this.ink,
    required this.muted,
    required this.accent,
    required this.secondary,
    required this.line,
    required this.radius,
  });

  final Color background, surface, ink, muted, accent, secondary, line;
  final double radius;

  factory UiThemeTokens.fromJson(Map<String, dynamic> data) {
    Color color(String key) {
      final value = data[key];
      if (value is! String || !RegExp(r'^#[0-9a-fA-F]{6}$').hasMatch(value)) {
        throw FormatException('Invalid theme color: $key');
      }
      return Color(0xff000000 | int.parse(value.substring(1), radix: 16));
    }

    final radius = data['radius'];
    if (radius is! num || !radius.isFinite || radius < 0 || radius > 32) {
      throw const FormatException('Invalid theme radius');
    }
    return UiThemeTokens(
      background: color('background'),
      surface: color('surface'),
      ink: color('ink'),
      muted: color('muted'),
      accent: color('accent'),
      secondary: color('secondary'),
      line: color('line'),
      radius: radius.toDouble(),
    );
  }
}
