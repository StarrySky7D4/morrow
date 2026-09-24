import 'surface_paint_boundary.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter/material.dart';
import 'media/texture_source.dart';
import 'liquid_glass.dart';
import 'neumorphic_controls.dart';

enum GlassMode { frosted, clear, liquid }

enum VisualStyle {
  flat,
  neumorphism,
  paper,
  clay,
  fluent,
  brutalist,
  industrial;

  bool get supportsDepth => this != flat;

  bool get experimental => this != flat && this != neumorphism;

  // Scale the existing radius preference rather than overwriting it. An
  // explicitly square theme stays square in every style.
  double get radiusScale => switch (this) {
    flat || neumorphism => 1,
    paper => .24,
    clay => 1.3,
    fluent => .55,
    brutalist => .18,
    industrial => .4,
  };
}

enum StudioTheme { white, custom, dark }

enum BackgroundMode { ambient, solid, texture, transparent }

@immutable
class ComponentMaterial {
  const ComponentMaterial({
    this.enabled = false,
    this.blur = 22,
    this.opacity = .76,
    this.color,
    this.mode,
    this.cornerRadius,
    this.styleDepth,
    this.followComponent,
  });
  final String? followComponent;
  final GlassMode? mode;
  final double? cornerRadius;
  final double? styleDepth;
  final bool enabled;
  final double blur, opacity;
  final Color? color;
  ComponentMaterial copyWith({
    bool? enabled,
    double? blur,
    double? opacity,
    Color? color,
    GlassMode? mode,
    double? cornerRadius,
    double? styleDepth,
    bool inheritDepth = false,
    String? followComponent,
    bool clearFollow = false,
    bool inheritColor = false,
    bool inheritMode = false,
    bool inheritRadius = false,
  }) => ComponentMaterial(
    enabled: enabled ?? this.enabled,
    blur: blur ?? this.blur,
    opacity: opacity ?? this.opacity,
    color: inheritColor ? null : color ?? this.color,
    mode: inheritMode ? null : mode ?? this.mode,
    cornerRadius: inheritRadius ? null : cornerRadius ?? this.cornerRadius,
    styleDepth: inheritDepth ? null : styleDepth ?? this.styleDepth,
    followComponent: clearFollow
        ? null
        : followComponent ?? this.followComponent,
  );
  Map<String, dynamic> toJson() => {
    'enabled': enabled,
    'blur': blur,
    'opacity': opacity,
    'color': color?.toARGB32(),
    if (mode != null) 'mode': mode!.name,
    if (cornerRadius != null) 'cornerRadius': cornerRadius,
    if (styleDepth != null) 'styleDepth': styleDepth,
    if (followComponent != null) 'followComponent': followComponent,
  };
  factory ComponentMaterial.fromJson(Map<String, dynamic> data) {
    final blur = (data['blur'] as num?)?.toDouble() ?? 22;
    final opacity = (data['opacity'] as num?)?.toDouble() ?? .76;
    final radius = (data['cornerRadius'] as num?)?.toDouble();
    final depth = (data['styleDepth'] as num?)?.toDouble();
    final modeName = data['mode'] as String?;
    final follow = data['followComponent'];
    if ((follow != null &&
            (follow is! String || follow.isEmpty || follow.length > 256)) ||
        (depth != null && (!depth.isFinite || depth < 0 || depth > 2)) ||
        (radius != null && (!radius.isFinite || radius < 0 || radius > 32)) ||
        (modeName != null &&
            !GlassMode.values.any((m) => m.name == modeName)) ||
        !blur.isFinite ||
        !opacity.isFinite ||
        blur < 0 ||
        blur > 40 ||
        opacity < 0 ||
        opacity > 1) {
      throw const FormatException('Invalid component material');
    }
    return ComponentMaterial(
      enabled: data['enabled'] as bool? ?? false,
      blur: blur,
      opacity: opacity,
      color: data['color'] == null ? null : Color(data['color'] as int),
      mode: modeName == null ? null : GlassMode.values.byName(modeName),
      cornerRadius: radius,
      styleDepth: depth,
      followComponent: follow as String?,
    );
  }
}

/// Independent material parameters; theme values remain authoritative by default.
@immutable
class SurfaceSettings {
  const SurfaceSettings({
    this.components = const {},
    this.visualStyle = VisualStyle.flat,
    this.styleDepth = 1,
    this.canvasBlur = 0,
    this.canvasOpacity = 0,
    this.canvasColor,
    this.componentCustom = false,
    this.componentBlur = 22,
    this.componentOpacity = .76,
    this.componentColor,
  });
  final Map<String, ComponentMaterial> components;
  final VisualStyle visualStyle;
  final double styleDepth;
  double depthFor(String? id) {
    final local = resolveComponent(id);
    return local?.enabled == true
        ? local!.styleDepth ?? styleDepth
        : styleDepth;
  }

  final double canvasBlur, canvasOpacity, componentBlur, componentOpacity;
  final Color? canvasColor, componentColor;
  final bool componentCustom;

  /// A missing target inherits the theme. Each hop is checked, so a cycle is
  /// never silently truncated.
  ComponentMaterial? resolveComponent(String? id) {
    if (id == null) return null;
    final visited = <String>{};
    var current = id;
    while (true) {
      if (!visited.add(current)) {
        throw const FormatException('Component material cycle');
      }
      final material = components[current];
      if (material == null) {
        return current == id ? null : const ComponentMaterial();
      }
      final target = material.followComponent;
      if (target == null) return material;
      current = target;
    }
  }

  SurfaceSettings copyWith({
    Map<String, ComponentMaterial>? components,
    VisualStyle? visualStyle,
    double? styleDepth,
    double? canvasBlur,
    double? canvasOpacity,
    Color? canvasColor,
    bool? componentCustom,
    double? componentBlur,
    double? componentOpacity,
    Color? componentColor,
  }) => SurfaceSettings(
    components: components ?? this.components,
    visualStyle: visualStyle ?? this.visualStyle,
    styleDepth: styleDepth ?? this.styleDepth,
    canvasBlur: canvasBlur ?? this.canvasBlur,
    canvasOpacity: canvasOpacity ?? this.canvasOpacity,
    canvasColor: canvasColor ?? this.canvasColor,
    componentCustom: componentCustom ?? this.componentCustom,
    componentBlur: componentBlur ?? this.componentBlur,
    componentOpacity: componentOpacity ?? this.componentOpacity,
    componentColor: componentColor ?? this.componentColor,
  );
  Map<String, dynamic> toJson() => {
    'visualStyle': visualStyle.name,
    'styleDepth': styleDepth,
    'componentMaterials': components.map(
      (key, value) => MapEntry(key, value.toJson()),
    ),
    'canvasBlur': canvasBlur,
    'canvasOpacity': canvasOpacity,
    'canvasColor': canvasColor?.toARGB32(),
    'componentCustom': componentCustom,
    'componentBlur': componentBlur,
    'componentOpacity': componentOpacity,
    'componentColor': componentColor?.toARGB32(),
  };
  factory SurfaceSettings.fromJson(Map<String, dynamic> data) {
    double value(String key, double fallback, double max) {
      final v = (data[key] as num?)?.toDouble() ?? fallback;
      if (!v.isFinite) throw const FormatException('Invalid material value');
      return v.clamp(0, max);
    }

    Color? color(String key) =>
        data[key] == null ? null : Color(data[key] as int).withValues(alpha: 1);
    final style = data['visualStyle'];
    if (style != null &&
        (style is! String ||
            !VisualStyle.values.any((value) => value.name == style))) {
      throw const FormatException('Invalid visual style');
    }
    final depth = (data['styleDepth'] as num?)?.toDouble() ?? 1;
    if (!depth.isFinite || depth < 0 || depth > 2) {
      throw const FormatException('Invalid style depth');
    }
    final settings = SurfaceSettings(
      styleDepth: depth,
      visualStyle: style == null
          ? VisualStyle.flat
          : VisualStyle.values.byName(style),
      components: (data['componentMaterials'] as Map<String, dynamic>? ?? {})
          .map(
            (key, value) => MapEntry(
              key,
              ComponentMaterial.fromJson(value as Map<String, dynamic>),
            ),
          ),
      canvasBlur: value('canvasBlur', 0, 40),
      canvasOpacity: value('canvasOpacity', 0, 1),
      canvasColor: color('canvasColor'),
      componentCustom: data['componentCustom'] as bool? ?? false,
      componentBlur: value('componentBlur', 22, 40),
      componentOpacity: value(
        'componentOpacity',
        (data['frostedOpacity'] as num?)?.toDouble() ?? .76,
        1,
      ),
      componentColor: color('componentColor'),
    );
    for (final id in settings.components.keys) {
      settings.resolveComponent(id);
    }
    return settings;
  }
}

class Palette {
  const Palette(
    this.theme,
    this.mode, [
    this.backdrop = BackgroundMode.ambient,
    this.solidTint = 0,
    this.frostedOpacity = .76,
    this.customColor,
    this.texture,
    this.mediaPlaying = true,
    this.cornerRadius = 20,
    this.grayscale = 0,
    this.themeLightness,
    this.windowRadius = 20,
    this.liquidCanvas = false,
    this.themeColor,
    this.surfaces = const SurfaceSettings(),
  ]);
  final SurfaceSettings surfaces;
  Palette withSurfaces(SurfaceSettings value) => Palette(
    theme,
    mode,
    backdrop,
    solidTint,
    frostedOpacity,
    customColor,
    texture,
    mediaPlaying,
    cornerRadius,
    grayscale,
    themeLightness,
    windowRadius,
    liquidCanvas,
    themeColor,
    value,
  );

  final StudioTheme theme;
  final GlassMode mode;
  final BackgroundMode backdrop;
  final int solidTint;
  final double frostedOpacity;
  final Color? customColor;
  final Color? themeColor;
  final TextureSource? texture;
  final bool mediaPlaying;
  final bool liquidCanvas;
  final double cornerRadius, grayscale, windowRadius;
  final double? themeLightness;
  BorderRadius borderRadius(double base) => BorderRadius.circular(
    base * cornerRadius.clamp(0, 32) / 20 * surfaces.visualStyle.radiusScale,
  );
  Color tone(Color color) {
    final gray = .2126 * color.r + .7152 * color.g + .0722 * color.b;
    return Color.lerp(
      color,
      Color.from(alpha: color.a, red: gray, green: gray, blue: gray),
      isCustom ? grayscale.clamp(0, 1) : 0,
    )!;
  }

  bool get isCustom => theme == StudioTheme.custom;
  double get lightness => switch (theme) {
    StudioTheme.white => .975,
    StudioTheme.custom => themeLightness ?? .885,
    StudioTheme.dark => .095,
  };
  bool get dark => isCustom ? lightness < .46 : theme == StudioTheme.dark;
  bool get clear => mode == GlassMode.clear;
  bool get liquid => mode == GlassMode.liquid;
  Color themeTint(Color color, [double amount = .08]) => tone(
    themeColor == null
        ? color
        : Color.lerp(color, themeColor!.withValues(alpha: 1), amount)!,
  );
  Color componentColor(Color original) =>
      themeColor == null ? tone(original) : accent;
  static double _contrast(Color a, Color b) {
    final x = a.computeLuminance(), y = b.computeLuminance();
    return ((x > y ? x : y) + .05) / ((x > y ? y : x) + .05);
  }

  Color get onAccent =>
      _contrast(accent, Colors.white) >= _contrast(accent, Colors.black)
      ? Colors.white
      : Colors.black;
  Color get ink => isCustom
      ? themeTint(dark ? Colors.white : Colors.black)
      : themeTint(dark ? const Color(0xFFF0EDF8) : const Color(0xFF302D43));
  Color get muted => isCustom
      ? Color.lerp(ink, background, .12)!
      : themeTint(dark ? const Color(0xFFB4AEC5) : const Color(0xFF777184));
  Color get accent {
    if (themeColor == null) {
      return tone(dark ? const Color(0xFFC0AFFA) : const Color(0xFF7662BA));
    }
    final selected = tone(themeColor!.withValues(alpha: 1));
    final base = background;
    if (_contrast(selected, base) >= 4.5) return selected;
    final target = _contrast(Colors.white, base) > _contrast(Colors.black, base)
        ? Colors.white
        : Colors.black;
    var low = 0.0, high = 1.0;
    for (var i = 0; i < 12; i++) {
      final mid = (low + high) / 2;
      if (_contrast(Color.lerp(selected, target, mid)!, base) >= 4.5) {
        high = mid;
      } else {
        low = mid;
      }
    }
    return Color.lerp(selected, target, high)!;
  }

  Color get background => isCustom
      ? themeTint(
          Color.from(
            alpha: 1,
            red: lightness,
            green: lightness,
            blue: lightness,
          ),
          .04,
        )
      : themeTint(switch (theme) {
          StudioTheme.white => const Color(0xFFF9F8FC),
          StudioTheme.custom => const Color(0xFFE1E3E9),
          StudioTheme.dark => const Color(0xFF181720),
        });
  Color get surface => isCustom
      ? themeTint(
          Color.lerp(
            background,
            dark ? Colors.white : Colors.black,
            dark ? .065 : .015,
          )!,
        )
      : themeTint(dark ? const Color(0xFF292634) : Colors.white);
  Color get solidColor =>
      customColor ??
      (solidTint == 0
          ? background
          : (dark
                ? const [
                    Color(0xFF181720),
                    Color(0xFF282437),
                    Color(0xFF20322D),
                    Color(0xFF342923),
                  ]
                : const [
                    Color(0xFFF9F8FC),
                    Color(0xFFE5DEF0),
                    Color(0xFFDDE9E2),
                    Color(0xFFF0E3D8),
                  ])[solidTint]);
  Color get line => ink.withValues(alpha: dark ? .13 : .075);
  Color get glassEdge => clear
      ? Color.lerp(
          surface,
          Colors.white,
          .75,
        )!.withValues(alpha: dark ? .28 : .50)
      : Color.lerp(
          backdrop == BackgroundMode.solid ? solidColor : surface,
          ink,
          .18,
        )!.withValues(alpha: .32 * frostedOpacity.clamp(.2, 1));

  Color get captionColor =>
      backdrop == BackgroundMode.transparent ||
          backdrop == BackgroundMode.texture
      ? Colors.transparent
      : (backdrop == BackgroundMode.solid ? solidColor : surface).withValues(
          alpha: clear ? .06 : frostedOpacity.clamp(.2, 1),
        );
}

// Style changes affect edges and depth only. Glass opacity, blur, tint and
// refraction remain independent, including component-specific overrides.
GlassMaterial _styleGlass(
  GlassMaterial inherited,
  Palette p, {
  required bool recessed,
}) {
  final style = p.surfaces.visualStyle;
  if (style == VisualStyle.flat) return inherited;
  final depth = p.surfaces.styleDepth;
  BoxShadow shadow(Color color, Offset offset, double blur) => BoxShadow(
    color: color.withValues(alpha: color.a * depth.clamp(0, 1)),
    offset: offset * depth,
    blurRadius: blur,
    // A shifted filled shadow must not darken a transparent panel's interior.
    blurStyle: BlurStyle.outer,
  );
  final darkEdge = p.ink.withValues(alpha: p.dark ? .28 : .18);
  final lightEdge = Colors.white.withValues(alpha: p.dark ? .1 : .58);
  final shadows = switch (style) {
    VisualStyle.flat => <BoxShadow>[],
    VisualStyle.neumorphism => [
      shadow(
        Colors.white.withValues(alpha: p.dark ? .09 : .26),
        const Offset(-1.5, -1.5),
        5,
      ),
      shadow(
        (p.dark ? Colors.black : const Color(0xFF8A8299)).withValues(
          alpha: p.dark ? .24 : .12,
        ),
        const Offset(1.5, 1.5),
        5,
      ),
    ],
    VisualStyle.paper => [shadow(darkEdge, const Offset(0, 2), 1)],
    VisualStyle.clay => [
      shadow(
        lightEdge.withValues(alpha: p.dark ? .08 : .26),
        const Offset(-1, -1.5),
        3,
      ),
      shadow(
        Colors.black.withValues(alpha: p.dark ? .28 : .13),
        const Offset(1, 2),
        4,
      ),
    ],
    VisualStyle.fluent => [
      shadow(
        Colors.black.withValues(alpha: p.dark ? .22 : .07),
        const Offset(0, 1.5),
        4,
      ),
    ],
    VisualStyle.brutalist => [
      shadow(
        p.ink.withValues(alpha: p.dark ? .48 : .65),
        const Offset(2.5, 2.5),
        0,
      ),
    ],
    VisualStyle.industrial => [
      shadow(lightEdge, const Offset(0, -1), 0),
      shadow(darkEdge, const Offset(0, 2), 1),
    ],
  };
  final edge = switch (style) {
    VisualStyle.neumorphism => Colors.white.withValues(
      alpha: p.dark ? .13 : .58,
    ),
    VisualStyle.clay => lightEdge,
    VisualStyle.brutalist => p.ink.withValues(alpha: .65),
    VisualStyle.paper || VisualStyle.industrial => darkEdge,
    _ => p.glassEdge,
  };
  return GlassMaterial(
    blur: inherited.blur,
    liquid: inherited.liquid,
    decoration: inherited.decoration.copyWith(
      boxShadow: recessed ? const [] : shadows,
      border: Border.all(
        color: edge,
        width: style == VisualStyle.brutalist ? 1.8 : 1,
      ),
    ),
  );
}

class Glass extends StatelessWidget {
  const Glass({
    super.key,
    required this.p,
    required this.child,
    this.radius = 20,
    this.dialog = false,
    this.componentId,
    this.recessed = false,
  });
  final Palette p;
  final Widget child;
  final double radius;
  final bool dialog;
  final String? componentId;
  final bool recessed;

  @override
  Widget build(BuildContext context) {
    final local = p.surfaces.resolveComponent(componentId);
    final custom = local?.enabled ?? false;
    final inheritedTint = dialog && p.backdrop == BackgroundMode.solid
        ? p.solidColor
        : p.surface;
    final tint = custom ? local!.color ?? inheritedTint : inheritedTint;
    final mode = custom ? local!.mode ?? p.mode : p.mode;
    final clear = mode == GlassMode.clear;
    final liquid = mode == GlassMode.liquid;
    final borderRadius = custom && local!.cornerRadius != null
        ? BorderRadius.circular(radius * local.cornerRadius! / 20)
        : p.borderRadius(radius);
    // Editors need a reading surface even when surrounding cards are clear.
    final readable = dialog || MediaQuery.highContrastOf(context);
    final top = clear
        ? (readable ? .86 : (p.dark ? .12 : .10))
        : p.frostedOpacity.clamp(.2, 1).toDouble();
    final bottom = clear
        ? (readable ? .80 : .025)
        : p.frostedOpacity.clamp(.2, 1).toDouble();
    final inherited = liquid
        ? GlassMaterial.liquid(
            tint: tint,
            dark: p.dark,
            readable: readable,
            borderRadius: borderRadius,
          )
        : GlassMaterial(
            blur: clear ? 1 : 22,
            liquid: 0,
            decoration: BoxDecoration(
              borderRadius: borderRadius,
              boxShadow: [
                BoxShadow(
                  color: (p.dark ? Colors.black : const Color(0xFF716386))
                      .withValues(alpha: clear ? .025 : .035),
                  blurRadius: clear ? 14 : 18,
                  offset: Offset(0, clear ? 4 : 8),
                ),
              ],
              gradient: LinearGradient(
                begin: Alignment.topLeft,
                end: Alignment.bottomRight,
                colors: [
                  tint.withValues(alpha: top),
                  tint.withValues(alpha: (top + bottom) / 2),
                  tint.withValues(alpha: bottom),
                ],
              ),
              border: Border.all(color: p.glassEdge, width: 1),
            ),
          );
    final styledPalette = p.withSurfaces(
      p.surfaces.copyWith(styleDepth: p.surfaces.depthFor(componentId)),
    );
    final styled = _styleGlass(inherited, styledPalette, recessed: recessed);
    final target = custom
        ? GlassMaterial(
            blur: local!.blur,
            liquid: styled.liquid,
            decoration: styled.decoration.copyWith(
              gradient: LinearGradient(
                begin: Alignment.topLeft,
                end: Alignment.bottomRight,
                colors: [
                  tint.withValues(
                    alpha: readable
                        ? local.opacity.clamp(.8, 1)
                        : local.opacity,
                  ),
                  tint.withValues(
                    alpha: readable
                        ? local.opacity.clamp(.8, 1)
                        : local.opacity,
                  ),
                ],
              ),
            ),
          )
        : styled;
    return TweenAnimationBuilder<GlassMaterial>(
      tween: GlassMaterialTween(end: target),
      duration: motionDuration(context, 360),
      curve: Curves.easeInOutCubic,
      child: child,
      builder: (context, material, child) => SurfacePaintBoundary(
        child: NeumorphicSurface(
          palette: styledPalette,
          depth: recessed ? -1 : 0,
          borderRadius: material.decoration.borderRadius! as BorderRadius,
          child: LiquidGlassSurface(
            material: material,
            tint: tint,
            dark: p.dark,
            readable: dialog,
            borderRadius: material.decoration.borderRadius! as BorderRadius,
            child: child!,
          ),
        ),
      ),
    );
  }
}

Duration motionDuration(BuildContext context, int milliseconds) =>
    MediaQuery.disableAnimationsOf(context)
    ? Duration.zero
    : Duration(milliseconds: milliseconds);

class AppearanceScope extends InheritedWidget {
  const AppearanceScope({
    super.key,
    required this.palette,
    required super.child,
  });
  final Palette palette;
  static Palette? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<AppearanceScope>()?.palette;
  static Palette of(BuildContext context) =>
      maybeOf(context) ?? const Palette(StudioTheme.white, GlassMode.frosted);
  @override
  bool updateShouldNotify(AppearanceScope oldWidget) =>
      oldWidget.palette != palette;
}

/// Shared by editors, pickers and detail dialogs. Media is never duplicated
/// inside a dialog: the existing canvas shows through the glass instead.
class StudioDialog extends StatelessWidget {
  const StudioDialog({
    super.key,
    required this.title,
    required this.content,
    this.subtitle,
    this.icon = Icons.auto_awesome_outlined,
    this.actions = const [],
    this.width = 460,
    this.canClose = true,
  });
  final String title;
  final String? subtitle;
  final IconData icon;
  final Widget content;
  final List<Widget> actions;
  final double width;
  final bool canClose;
  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final height =
        (MediaQuery.sizeOf(context).height -
                MediaQuery.viewInsetsOf(context).bottom -
                40)
            .clamp(160.0, 760.0);
    return Dialog(
      key: const ValueKey('studio-dialog'),
      backgroundColor: Colors.transparent,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      insetPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 20),
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: width, maxHeight: height),
        child: Glass(
          p: p,
          dialog: true,
          radius: 28,
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Container(
                      width: 38,
                      height: 38,
                      decoration: BoxDecoration(
                        color: p.accent.withValues(alpha: .15),
                        borderRadius: p.borderRadius(13),
                      ),
                      child: Icon(icon, color: p.accent, size: 21),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'MORROW / A LITTLE POSSIBILITY',
                            style: TextStyle(
                              color: p.muted,
                              fontSize: 8,
                              letterSpacing: 1,
                            ),
                          ),
                          const SizedBox(height: 5),
                          Text(
                            title,
                            style: TextStyle(
                              color: p.ink,
                              fontSize: 20,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ],
                      ),
                    ),
                    IconButton(
                      tooltip: L10n.of(context).visualCloseDialog,
                      onPressed: canClose ? () => Navigator.pop(context) : null,
                      icon: Icon(Icons.close_rounded, size: 18, color: p.muted),
                    ),
                  ],
                ),
                if (subtitle != null) ...[
                  const SizedBox(height: 12),
                  Text(
                    subtitle!,
                    style: TextStyle(color: p.muted, fontSize: 11, height: 1.7),
                  ),
                ],
                const SizedBox(height: 22),
                Flexible(child: SingleChildScrollView(child: content)),
                if (actions.isNotEmpty) ...[
                  const SizedBox(height: 20),
                  Divider(height: 1, color: p.line),
                  const SizedBox(height: 16),
                  Wrap(
                    alignment: WrapAlignment.end,
                    spacing: 8,
                    runSpacing: 8,
                    children: actions,
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

Future<T?> showStudioDialog<T>({
  required BuildContext context,
  required WidgetBuilder builder,
}) => showGeneralDialog<T>(
  context: context,
  barrierDismissible: true,
  barrierLabel: L10n.of(context).visualCloseDialog,
  barrierColor: AppearanceScope.of(context).backdrop == BackgroundMode.texture
      ? Colors.black.withValues(alpha: .12)
      : Colors.black.withValues(alpha: .20),
  transitionDuration: motionDuration(context, 260),
  pageBuilder: (context, _, _) => builder(context),
  transitionBuilder: (context, animation, _, child) => FadeTransition(
    opacity: animation,
    child: ScaleTransition(
      scale: Tween<double>(
        begin: .96,
        end: 1,
      ).animate(CurvedAnimation(parent: animation, curve: Curves.easeOutCubic)),
      child: child,
    ),
  ),
);

class SoftSwap extends StatelessWidget {
  const SoftSwap({super.key, required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => AnimatedSwitcher(
    duration: motionDuration(context, 220),
    switchInCurve: Curves.easeOutCubic,
    layoutBuilder: (current, previous) => Stack(
      alignment: Alignment.center,
      children: [
        ...previous.map(
          (child) => IgnorePointer(child: ExcludeSemantics(child: child)),
        ),
        ?current,
      ],
    ),
    transitionBuilder: (child, animation) => FadeTransition(
      opacity: animation,
      child: ScaleTransition(
        scale: Tween<double>(begin: .9, end: 1).animate(animation),
        child: child,
      ),
    ),
    child: child,
  );
}

class SoftSize extends StatelessWidget {
  const SoftSize({
    super.key,
    required this.child,
    required this.duration,
    this.alignment = Alignment.center,
  });
  final Widget child;
  final Duration duration;
  final Alignment alignment;
  @override
  Widget build(BuildContext context) => MediaQuery.disableAnimationsOf(context)
      ? child
      : AnimatedSize(duration: duration, alignment: alignment, child: child);
}
