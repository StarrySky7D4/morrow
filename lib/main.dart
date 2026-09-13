import 'plugins/query_coordinator.dart';
import 'plugins/editor_session.dart';
import 'plugins/plugin_tools.dart';
import 'plugins/protection_backup.dart';
import 'plugins/bootstrap_stub.dart'
    if (dart.library.io) 'plugins/bootstrap_native.dart'
    as bootstrap;
import 'plugins/studio_backend.dart';
import 'plugins/workbench_backend.dart';
import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:file_selector/file_selector.dart';
import 'package:super_clipboard/super_clipboard.dart';
import 'attachments/attachment.dart';
import 'attachments/attachment_view.dart';
import 'attachments/clipboard_import.dart';
import 'music/music_controller.dart';
import 'music/music_panel.dart';
import 'little_tips.dart';
import 'dart:math' as math;
import 'appearance.dart';
import 'component_material_page.dart';
import 'collapsible_panel.dart';
import 'settings_page_transition.dart';
import 'liquid_glass.dart';
import 'content/idea_markdown.dart';
export 'appearance.dart';
import 'color_compass.dart';
import 'desktop_frame.dart';
import 'media/texture_source.dart';
import 'media/texture_repository.dart';
import 'media/texture_backdrop.dart';
import 'media/texture_link_dialog.dart';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'storage.dart';
import 'storage_migration.dart';
import 'window_effects.dart';

part 'pages/workspace_pages.dart';

Future<void> main(List<String> arguments) async {
  if (await bootstrap.startRustWorkbench(arguments)) return;
  WidgetsFlutterBinding.ensureInitialized();
  await initializeDesktopFrame();
  StudioStorage storage;
  String? warning;
  try {
    await migrateLegacyStorage();
    storage = LocalStorage(await SharedPreferences.getInstance());
  } catch (_) {
    storage = MemoryStorage();
    warning = '本地存储暂不可用，当前改动仅保留在本次会话。';
  }
  runApp(
    MorrowApp(
      storage: storage,
      nativeBackground: DesktopBackground(),
      initialWarning: warning,
    ),
  );
}

class MorrowApp extends StatefulWidget {
  const MorrowApp({
    super.key,
    this.storage,
    this.nativeBackground,
    this.initialWarning,
    this.workbench,
  });
  final StudioStorage? storage;
  final DesktopBackground? nativeBackground;
  final String? initialWarning;
  final WorkbenchBackend? workbench;
  @override
  State<MorrowApp> createState() => _MorrowAppState();
}

class _MorrowAppState extends State<MorrowApp> {
  StudioTheme theme = StudioTheme.white;
  GlassMode mode = GlassMode.frosted;
  BackgroundMode background = BackgroundMode.ambient;
  int solidTint = 0;
  double frostedOpacity = .76;
  double cornerRadius = 20, windowRadius = 20, grayscale = 0;
  double? themeLightness;
  Color? customColor;
  Color? themeColor;
  Color? committedThemeColor;
  TextureSource? texture;
  bool mediaPlaying = true;
  bool liquidCanvas = false;
  SurfaceSettings surfaces = const SurfaceSettings();
  SurfaceSettings committedSurfaces = const SurfaceSettings();
  late final StudioStorage storage;
  Map<String, dynamic>? restored;
  String? warning;
  final messages = GlobalKey<ScaffoldMessengerState>();

  @override
  void initState() {
    super.initState();
    storage = widget.storage ?? MemoryStorage();
    warning = widget.initialWarning;
    try {
      restored = storage.read();
      final data = restored;
      if (data != null) {
        theme = StudioTheme.values.byName(
          data['theme'] == 'mist' ? 'custom' : data['theme'] as String,
        );
        mode = GlassMode.values.byName(data['glass'] as String);
        background = BackgroundMode.values.byName(data['background'] as String);
        solidTint = (data['solidTint'] as int? ?? 0).clamp(0, 3);
        frostedOpacity = ((data['frostedOpacity'] as num?)?.toDouble() ?? .76)
            .clamp(.2, 1);
        customColor = data['customColor'] == null
            ? null
            : Color(data['customColor'] as int);
        final savedThemeColor = data['themeColor'];
        themeColor =
            savedThemeColor is int &&
                savedThemeColor >= 0 &&
                savedThemeColor <= 0xffffffff
            ? Color(savedThemeColor).withValues(alpha: 1)
            : null;
        committedThemeColor = themeColor;
        texture = data['texture'] == null
            ? null
            : TextureSource.fromJson(data['texture'] as Map<String, dynamic>);
        mediaPlaying = data['mediaPlaying'] as bool? ?? true;
        liquidCanvas = data['liquidCanvas'] as bool? ?? false;
        surfaces = SurfaceSettings.fromJson(data);
        committedSurfaces = surfaces;
        themeLightness = (data['themeLightness'] as num?)?.toDouble().clamp(
          0,
          1,
        );
        cornerRadius = ((data['cornerRadius'] as num?)?.toDouble() ?? 20).clamp(
          0,
          32,
        );
        if (data['rounded'] == false) cornerRadius = 0;
        windowRadius = ((data['windowRadius'] as num?)?.toDouble() ?? 20).clamp(
          0,
          32,
        );
        grayscale = ((data['grayscale'] as num?)?.toDouble() ?? 0).clamp(0, 1);
        for (final track in (data['music']?['tracks'] as List? ?? [])) {
          MusicTrack.fromJson(track as Map<String, dynamic>);
        }
        // Validate content before restoring any of it; retain unreadable data.
        for (final raw in data['ideas'] as List) {
          Idea.fromJson(raw as Map<String, dynamic>);
        }
      }
    } catch (_) {
      restored = null;
      warning = '存储内容无法读取，原数据仍保留。当前会话不会覆盖它。';
      storageReadFailed = true;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (warning != null) {
        messages.currentState?.showSnackBar(
          SnackBar(
            content: Text(warning!),
            duration: const Duration(seconds: 8),
          ),
        );
      }
      applyWindowBackground();
    });
  }

  bool storageReadFailed = false;
  Future<void> saveContent(Map<String, dynamic> content) async {
    restored = content;
    if (storageReadFailed) return;
    try {
      await storage.write({
        ...content,
        'version': 1,
        'theme': theme.name,
        'glass': mode.name,
        'background': background.name,
        'solidTint': solidTint,
        'frostedOpacity': frostedOpacity,
        'customColor': customColor?.toARGB32(),
        'themeColor': committedThemeColor?.toARGB32(),
        'texture': texture?.toJson(),
        'mediaPlaying': mediaPlaying,
        'liquidCanvas': liquidCanvas,
        ...committedSurfaces.toJson(),
        'themeLightness': themeLightness,
        'windowRadius': windowRadius,
        'cornerRadius': cornerRadius,
        'grayscale': grayscale,
      });
    } catch (_) {
      if (mounted) {
        messages.currentState?.hideCurrentSnackBar();
        messages.currentState?.showSnackBar(
          SnackBar(
            content: const Text('保存失败，改动仍在当前会话中。'),
            action: SnackBarAction(
              label: '重试',
              onPressed: () => saveContent(restored ?? content),
            ),
          ),
        );
      }
    }
  }

  bool backgroundWarningShown = false;
  Future<void> applyWindowBackground() async {
    try {
      await widget.nativeBackground?.apply(
        frost: background == BackgroundMode.transparent
            ? surfaces.canvasBlur
            : 0,
      );
      backgroundWarningShown = false;
    } catch (error) {
      if (mounted && !backgroundWarningShown) {
        backgroundWarningShown = true;
        messages.currentState?.showSnackBar(
          SnackBar(
            content: Text(
              error is PlatformException && error.code == 'backdrop_unavailable'
                  ? '当前系统无法启用桌面磨砂，染色和透明度仍可调整。'
                  : '系统透明效果未能启用，可切换到默认背景继续使用。',
            ),
          ),
        );
      }
    }
  }

  void appearanceChanged(
    VoidCallback update, {
    bool native = false,
    bool save = true,
  }) {
    setState(update);
    if (save) saveContent(restored ?? {'ideas': [], 'completed': <String>[]});
    if (native) applyWindowBackground();
  }

  @override
  Widget build(BuildContext context) {
    final palette = Palette(
      theme,
      mode,
      background,
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
      surfaces,
    );
    return MaterialApp(
      title: 'Morrow — 留一点空间给灵感',
      debugShowCheckedModeBanner: false,
      scaffoldMessengerKey: messages,
      builder: (_, child) => AppearanceScope(
        palette: palette,
        child: AnnotatedRegion<SystemUiOverlayStyle>(
          value:
              (palette.dark
                      ? SystemUiOverlayStyle.light
                      : SystemUiOverlayStyle.dark)
                  .copyWith(
                    statusBarColor: Colors.transparent,
                    systemNavigationBarColor: Colors.transparent,
                    systemNavigationBarContrastEnforced: false,
                  ),
          child: widget.nativeBackground != null && isWindowsDesktop
              ? DesktopFrame(palette: palette, child: child!)
              : child!,
        ),
      ),
      theme: ThemeData(
        useMaterial3: true,
        brightness: palette.dark ? Brightness.dark : Brightness.light,
        fontFamily: 'Segoe UI',
        fontFamilyFallback: const ['Microsoft YaHei', 'Arial'],
        scaffoldBackgroundColor: Colors.transparent,
        filledButtonTheme: FilledButtonThemeData(
          style: FilledButton.styleFrom(
            shape: RoundedRectangleBorder(
              borderRadius: palette.borderRadius(12),
            ),
          ),
        ),
        outlinedButtonTheme: OutlinedButtonThemeData(
          style: OutlinedButton.styleFrom(
            shape: RoundedRectangleBorder(
              borderRadius: palette.borderRadius(12),
            ),
          ),
        ),
        textButtonTheme: TextButtonThemeData(
          style: TextButton.styleFrom(
            shape: RoundedRectangleBorder(
              borderRadius: palette.borderRadius(10),
            ),
          ),
        ),
        iconButtonTheme: IconButtonThemeData(
          style: IconButton.styleFrom(
            shape: RoundedRectangleBorder(
              borderRadius: palette.borderRadius(12),
            ),
          ),
        ),
        inputDecorationTheme: InputDecorationTheme(
          filled: true,
          fillColor: palette.surface.withValues(alpha: .13),
          contentPadding: const EdgeInsets.symmetric(
            horizontal: 14,
            vertical: 15,
          ),
          labelStyle: TextStyle(color: palette.muted, fontSize: 12),
          enabledBorder: OutlineInputBorder(
            borderRadius: palette.borderRadius(13),
            borderSide: BorderSide(color: palette.line),
          ),
          focusedBorder: OutlineInputBorder(
            borderRadius: palette.borderRadius(13),
            borderSide: BorderSide(color: palette.accent),
          ),
          border: OutlineInputBorder(borderRadius: palette.borderRadius(13)),
        ),
        popupMenuTheme: PopupMenuThemeData(
          color: palette.surface.withValues(alpha: .95),
          shape: RoundedRectangleBorder(borderRadius: palette.borderRadius(16)),
          textStyle: TextStyle(color: palette.ink, fontSize: 12),
        ),
        colorScheme: ColorScheme.fromSeed(
          seedColor: palette.accent,
          primary: palette.accent,
          onPrimary: palette.onAccent,
          secondary: palette.accent,
          onSecondary: palette.onAccent,
          tertiary: palette.accent,
          onTertiary: palette.onAccent,
          surface: palette.surface,
          onSurface: palette.ink,
          brightness: palette.dark ? Brightness.dark : Brightness.light,
        ),
        textTheme: TextTheme(
          bodyMedium: TextStyle(color: palette.ink, fontSize: 13),
          bodySmall: TextStyle(color: palette.muted, fontSize: 12),
        ),
        tooltipTheme: const TooltipThemeData(
          waitDuration: Duration(milliseconds: 400),
        ),
      ),
      home: Studio(
        desktopCaption: widget.nativeBackground != null && isWindowsDesktop,
        palette: palette,
        onTheme: (value) =>
            appearanceChanged(() => theme = value, native: true),
        onLightness: (value) {
          if (theme != StudioTheme.custom) return;
          appearanceChanged(() => themeLightness = value, save: false);
        },
        onWindowRadius: (value) =>
            appearanceChanged(() => windowRadius = value, save: false),
        onRadius: (value) =>
            appearanceChanged(() => cornerRadius = value, save: false),
        onGrayscale: (value) {
          if (theme != StudioTheme.custom) return;
          appearanceChanged(() => grayscale = value, save: false);
        },
        onMode: (value) => appearanceChanged(() => mode = value),
        onSurfaces: (value) => appearanceChanged(
          () => surfaces = value,
          save: false,
          native: true,
        ),
        onLiquidCanvas: (value) =>
            appearanceChanged(() => liquidCanvas = value),
        onBackground: (value) =>
            appearanceChanged(() => background = value, native: true),
        onTint: (value) => appearanceChanged(() {
          solidTint = value;
          customColor = null;
        }),
        onOpacity: (value) => appearanceChanged(
          () => frostedOpacity = value.clamp(.2, 1),
          save: false,
        ),
        onAppearanceCommit: () {
          committedThemeColor = themeColor;
          committedSurfaces = surfaces;
          saveContent(restored ?? {'ideas': [], 'completed': <String>[]});
          applyWindowBackground();
        },
        onColor: (value) => appearanceChanged(() => customColor = value),
        onThemeColor: (value) =>
            appearanceChanged(() => themeColor = value, save: false),
        onTexture: (value) => appearanceChanged(() => texture = value),
        onPlaying: (value) => appearanceChanged(() => mediaPlaying = value),
        workbench: widget.workbench,
        restored: restored,
        onSave: saveContent,
        onReady: (data) => restored = data,
      ),
    );
  }
}

class Idea {
  Idea(
    this.title,
    this.description,
    this.category,
    this.icon,
    this.color, {
    this.favorite = false,
    this.time = '刚刚',
    this.todos = const [],
    String? id,
    Set<String>? completed,
    this.attachments = const [],
    String? stage,
    this.hypothesis = '',
    this.conclusion = '',
  }) : id = id ?? nextId(),
       completed = completed ?? {},
       stage =
           stage ??
           (category == '进行中'
               ? '推进中'
               : category == '实验'
               ? '待验证'
               : '待整理');
  static int _sequence = 0;
  static String nextId() =>
      '${DateTime.now().microsecondsSinceEpoch}-${_sequence++}';
  final String id;
  String title, description, category, stage, hypothesis, conclusion;
  final List<IdeaAttachment> attachments;
  final String time;
  final IconData icon;
  final Color color;
  final List<String> todos;
  bool favorite;
  final Set<String> completed;
  static const icons = [
    Icons.auto_awesome_outlined,
    Icons.spa_outlined,
    Icons.blur_on_rounded,
    Icons.widgets_outlined,
  ];
  Map<String, dynamic> toJson() => {
    'id': id,
    'title': title,
    'description': description,
    'category': category,
    'time': time,
    'icon': icons.indexOf(icon),
    'color': color.toARGB32(),
    'favorite': favorite,
    'todos': todos,
    'completed': completed.toList(),
    'stage': stage,
    'hypothesis': hypothesis,
    'conclusion': conclusion,
    'attachments': attachments.map((a) => a.toJson()).toList(),
  };
  factory Idea.fromJson(Map<String, dynamic> data) => Idea(
    data['title'] as String,
    data['description'] as String,
    data['category'] as String,
    icons[(data['icon'] as int).clamp(0, icons.length - 1)],
    Color(data['color'] as int),
    id: data['id'] as String,
    time: data['time'] as String,
    favorite: data['favorite'] as bool,
    todos: List<String>.from(data['todos'] as List),
    completed: Set<String>.from(data['completed'] as List),
    stage:
        data['stage'] as String? ??
        (data['category'] == '进行中'
            ? '推进中'
            : data['category'] == '实验'
            ? '待验证'
            : '待整理'),
    hypothesis: data['hypothesis'] as String? ?? '',
    conclusion: data['conclusion'] as String? ?? '',
    attachments: (data['attachments'] as List? ?? [])
        .map((a) => IdeaAttachment.fromJson(a as Map<String, dynamic>))
        .toList(),
  );
}

class Studio extends StatefulWidget {
  const Studio({
    super.key,
    required this.palette,
    required this.onTheme,
    required this.onMode,
    required this.onLiquidCanvas,
    required this.onWindowRadius,
    required this.onRadius,
    required this.onGrayscale,
    required this.onLightness,
    required this.onBackground,
    required this.onTint,
    required this.onOpacity,
    required this.onAppearanceCommit,
    required this.onColor,
    required this.onThemeColor,
    required this.onTexture,
    required this.onPlaying,
    required this.onSave,
    required this.onReady,
    this.restored,
    this.workbench,
    this.desktopCaption = false,
    this.onSurfaces,
  });
  final ValueChanged<SurfaceSettings>? onSurfaces;
  final WorkbenchBackend? workbench;
  final bool desktopCaption;
  final Palette palette;
  final ValueChanged<StudioTheme> onTheme;
  final ValueChanged<GlassMode> onMode;
  final ValueChanged<bool> onLiquidCanvas;
  final ValueChanged<double> onRadius, onWindowRadius, onGrayscale, onLightness;
  final ValueChanged<BackgroundMode> onBackground;
  final ValueChanged<int> onTint;
  final ValueChanged<double> onOpacity;
  final VoidCallback onAppearanceCommit;
  final ValueChanged<Color> onColor;
  final ValueChanged<Color?> onThemeColor;
  final ValueChanged<TextureSource?> onTexture;
  final ValueChanged<bool> onPlaying;
  final ValueChanged<Map<String, dynamic>> onSave, onReady;
  final Map<String, dynamic>? restored;
  @override
  State<Studio> createState() => _StudioState();
}

class _StudioState extends State<Studio> {
  String section = '概览';
  String filter = '全部';
  String query = '';
  final search = TextEditingController();
  final quickNote = TextEditingController();
  final searchFocus = FocusNode();
  bool showAppearance = true;
  bool compactSettingsOpen = false;
  bool sidebarExpanded = true;
  bool showCustomTone = false;
  late final MusicController music;
  bool backgroundSound = false;
  bool backgroundAudible = false;
  int soundRevision = 0;
  bool get soundRequested =>
      backgroundSound &&
      p.backdrop == BackgroundMode.texture &&
      p.texture?.kind == TextureKind.video &&
      p.mediaPlaying &&
      mediaError == null;
  Future<void> setBackgroundSound(bool value) async {
    final revision = ++soundRevision;
    if (value) await music.setBlocked(true);
    if (!mounted || revision != soundRevision) return;
    setState(() => backgroundSound = value);
    if (!value) await music.setBlocked(false);
  }

  Widget musicPanel() => AnimatedSwitcher(
    duration: motionDuration(context, 250),
    child: backgroundAudible
        ? const SizedBox.shrink(key: ValueKey('music-hidden-for-background'))
        : MusicPanel(key: const ValueKey('music-panel'), controller: music),
  );
  final completed = <String>{};
  String sort = '最近添加';
  bool importing = false;
  String? mediaError;
  final ideas = <Idea>[
    Idea(
      '给灵感一个容器',
      '把突然冒出的念头放在这里。\n不急着完成，先让它发生。',
      '灵感',
      Icons.auto_awesome_outlined,
      const Color(0xFF9D87D4),
      favorite: true,
      time: '10 分钟前',
    ),
    Idea(
      '一个安静的数字花园',
      '用小小的网页，收藏喜欢的文字、\n音乐和生活里的细枝末节。',
      '进行中',
      Icons.spa_outlined,
      const Color(0xFF83A997),
      favorite: true,
      time: '1 小时前',
      todos: ['整理第一批收藏', '设计花园入口', '种下一条新想法'],
    ),
    Idea(
      '周末，做点无用的东西',
      '试试生成艺术，让代码长出\n意料之外的形状。',
      '实验',
      Icons.blur_on_rounded,
      const Color(0xFFB79277),
      time: '3 小时前',
    ),
    Idea(
      '我的桌面小助手',
      '一个低调常驻的伙伴，帮我记住\n那些容易忘记的小事。',
      '进行中',
      Icons.widgets_outlined,
      const Color(0xFF839AC0),
      time: '昨天',
      todos: ['画一个小小的原型', '定义提醒交互'],
    ),
  ];
  Palette get p => widget.palette;

  Widget mediaCanvas() {
    final source = p.backdrop == BackgroundMode.texture ? p.texture : null;
    return AnimatedSwitcher(
      duration: motionDuration(context, 280),
      // Remove outgoing media immediately so a fading video cannot keep its
      // audio playing over a newly selected source. The new canvas fades in.
      layoutBuilder: (current, previous) =>
          Stack(fit: StackFit.expand, children: [?current]),
      child: source == null
          ? const SizedBox.expand(key: ValueKey('no-media'))
          : TextureBackdrop(
              key: ValueKey(
                '${source.local}/${source.location}/${source.kind}',
              ),
              source: source,
              playing: p.mediaPlaying,
              muted: !soundRequested,
              onAudible: (value) {
                if (!mounted || p.texture?.location != source.location) return;
                if (backgroundAudible != value) {
                  setState(() => backgroundAudible = value);
                }
                music.setBlocked(value);
              },
              onError: (error) {
                if (mounted &&
                    p.texture?.location == source.location &&
                    mediaError != error) {
                  setState(() {
                    mediaError = error;
                    if (error != null) backgroundAudible = false;
                  });
                  if (error != null) music.setBlocked(false);
                }
              },
            ),
    );
  }

  Future<void> chooseTexture({bool online = false}) async {
    if (importing) return;
    setState(() => importing = !online);
    try {
      final source = online
          ? await showStudioDialog<TextureSource>(
              context: context,
              builder: (_) => const TextureLinkDialog(),
            )
          : await TextureRepository.pick();
      if (source != null && mounted) {
        setState(() => mediaError = null);
        widget.onTexture(source);
      }
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              error is FormatException ? error.message : '素材导入失败，请检查文件与可用存储空间。',
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => importing = false);
    }
  }

  Future<void> chooseThemeColor() async {
    final previous = p.themeColor;
    final color = await showStudioDialog<Color>(
      context: context,
      builder: (_) => ColorCompassDialog(
        initial: previous ?? p.accent,
        title: '主题色彩色罗盘',
        onChanged: widget.onThemeColor,
      ),
    );
    if (!mounted) return;
    widget.onThemeColor(color ?? previous);
    if (color != null) widget.onAppearanceCommit();
  }

  Future<void> chooseColor() async {
    final color = await showStudioDialog<Color>(
      context: context,
      builder: (_) => ColorCompassDialog(initial: p.solidColor),
    );
    if (color != null && mounted) widget.onColor(color);
  }

  @override
  void initState() {
    super.initState();
    search.addListener(() {
      final composing = search.value.composing;
      final active = composing.isValid && !composing.isCollapsed;
      if (query != search.text || _searchComposing != active) {
        setState(() {
          query = search.text;
          _searchComposing = active;
          pluginProjection();
        });
      }
    });
    sidebarExpanded = widget.restored?['sidebarExpanded'] as bool? ?? true;
    showAppearance = widget.restored?['appearanceExpanded'] as bool? ?? true;
    final savedMusic = widget.restored?['music'] as Map<String, dynamic>?;
    music = MusicController(
      plugin: widget.workbench?.studio,
      tracks: (savedMusic?['tracks'] as List? ?? [])
          .map((raw) => MusicTrack.fromJson(raw as Map<String, dynamic>))
          .toList(),
      index: savedMusic?['index'] as int? ?? 0,
      showLyrics: savedMusic?['showLyrics'] as bool? ?? false,
      onlineLyrics: savedMusic?['onlineLyrics'] as bool? ?? true,
      onSave: persist,
    );
    if (widget.restored != null) {
      ideas
        ..clear()
        ..addAll(
          (widget.restored!['ideas'] as List).map(
            (raw) => Idea.fromJson(raw as Map<String, dynamic>),
          ),
        );
      completed.addAll(
        List<String>.from(widget.restored!['completed'] as List? ?? []),
      );
    }
    widget.onReady(snapshot());
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final width =
        MediaQuery.sizeOf(context).width -
        MediaQuery.paddingOf(context).horizontal;
    if (width >= 1050) compactSettingsOpen = false;
  }

  void closeCompactSettings() {
    FocusManager.instance.primaryFocus?.unfocus();
    setState(() => compactSettingsOpen = false);
  }

  Widget compactSettings() => Focus(
    autofocus: true,
    child: Padding(
      key: const ValueKey('compact-settings-page'),
      padding: const EdgeInsets.all(12),
      child: Column(
        children: [
          Row(
            children: [
              IconButton(
                key: const ValueKey('compact-settings-back'),
                tooltip: '返回工作台',
                onPressed: closeCompactSettings,
                icon: Icon(Icons.arrow_back_rounded, color: p.ink),
              ),
              const SizedBox(width: 8),
              Text(
                '设置',
                style: TextStyle(
                  color: p.ink,
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Expanded(
            child: SingleChildScrollView(
              key: const PageStorageKey('compact-settings-scroll'),
              child: Column(
                children: [
                  appearance(),
                  const SizedBox(height: 20),
                  scratchpad(),
                  const SizedBox(height: 20),
                  musicPanel(),
                  const SizedBox(height: 20),
                ],
              ),
            ),
          ),
        ],
      ),
    ),
  );

  Map<String, dynamic> snapshot() => {
    'ideas': ideas.map((idea) => idea.toJson()).toList(),
    'completed': completed.toList(),
    'music': music.toJson(),
    'sidebarExpanded': sidebarExpanded,
    'appearanceExpanded': showAppearance,
  };
  void persist() => widget.onSave(snapshot());
  void refreshPage(VoidCallback update) => setState(update);

  @override
  void dispose() {
    _queries.dispose();
    music.dispose();
    search.dispose();
    quickNote.dispose();
    searchFocus.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(Studio oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!soundRequested) {
      backgroundAudible = false;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) music.setBlocked(false);
      });
    }
  }

  bool _pluginBusy = false;
  int _contentGeneration = 0;
  late final _queries = QueryCoordinator(
    onChanged: () {
      if (mounted) setState(() {});
    },
  );
  bool _searchComposing = false;
  Future<Idea?> pluginChange(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
    int? position,
  }) async {
    final backend = widget.workbench;
    if (backend == null || _pluginBusy) return null;
    _pluginBusy = true;
    try {
      final result = await backend.apply(action, idea, text: text, flag: flag);
      if (!mounted) return result;
      setState(() {
        final index = ideas.indexWhere((item) => item.id == result.id);
        if (action == PluginAction.delete) {
          if (index >= 0) ideas.removeAt(index);
        } else if (index >= 0) {
          ideas[index] = result;
        } else {
          ideas.insert((position ?? 0).clamp(0, ideas.length), result);
        }
        _contentGeneration++;
      });
      persist();
      return result;
    } catch (_) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: const Text('这次修改未能保存，草稿保留，可重试。'),
            duration: const Duration(seconds: 30),
            action: SnackBarAction(
              label: '重试',
              onPressed: () => pluginChange(
                action,
                idea,
                text: text,
                flag: flag,
                position: position,
              ),
            ),
          ),
        );
      }
      return null;
    } finally {
      _pluginBusy = false;
    }
  }

  void pluginProjection() {
    _queries.select(widget.workbench, (
      contentGeneration: _contentGeneration,
      section: section,
      filter: filter,
      text: query,
      sort: sort,
    ), deferred: _searchComposing);
  }

  List<Idea> get visibleIdeas {
    pluginProjection();
    if (widget.workbench?.writable ?? false) {
      final byId = {for (final idea in ideas) idea.id: idea};
      return [
        for (final id in _queries.ids)
          if (byId[id] != null) byId[id]!,
      ];
    }

    final result = ideas.where((idea) {
      final sectionMatches = switch (section) {
        '灵感收件箱' => idea.category == '灵感',
        '小项目' => idea.category == '进行中',
        '实验室' => idea.category == '实验',
        '已收藏' => idea.favorite,
        _ => true,
      };
      return sectionMatches &&
          matchesPageFilter(idea) &&
          '${idea.title} ${idea.description} ${idea.hypothesis} ${idea.conclusion} ${idea.attachments.map((a) => a.source.name).join(' ')}'
              .toLowerCase()
              .contains(query.toLowerCase());
    }).toList();
    if (sort == '标题排序') result.sort((a, b) => a.title.compareTo(b.title));
    if (sort == '收藏优先') {
      result.sort((a, b) => (b.favorite ? 1 : 0).compareTo(a.favorite ? 1 : 0));
    }
    return result;
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: !compactSettingsOpen,
      onPopInvokedWithResult: (didPop, result) {
        if (!didPop && compactSettingsOpen) closeCompactSettings();
      },
      child: CallbackShortcuts(
        bindings: {
          if (compactSettingsOpen)
            const SingleActivator(LogicalKeyboardKey.escape):
                closeCompactSettings,
          if (!compactSettingsOpen) ...{
            const SingleActivator(LogicalKeyboardKey.keyK, control: true): () =>
                searchFocus.requestFocus(),
            const SingleActivator(LogicalKeyboardKey.keyN, control: true):
                createIdea,
          },
        },
        child: Scaffold(
          body: Stack(
            children: [
              // Keep opaque modes opaque while their foregrounds crossfade.
              // This fill is inside DesktopFrame's clip, never in the native host.
              Positioned.fill(
                child: ColoredBox(
                  color: p.backdrop == BackgroundMode.transparent
                      ? (!kIsWeb &&
                                defaultTargetPlatform == TargetPlatform.android
                            ? p.background
                            : Colors.transparent)
                      : p.backdrop == BackgroundMode.solid
                      ? p.solidColor
                      : p.background,
                ),
              ),
              Positioned.fill(
                child: AnimatedSwitcher(
                  duration: MediaQuery.disableAnimationsOf(context)
                      ? Duration.zero
                      : const Duration(milliseconds: 360),
                  child: RepaintBoundary(
                    key: ValueKey(
                      '${p.theme.name}-${p.backdrop.name}-${p.solidColor.toARGB32()}',
                    ),
                    child: SizedBox.expand(
                      child: CustomPaint(painter: AmbientPainter(p)),
                    ),
                  ),
                ),
              ),
              Positioned.fill(child: mediaCanvas()),
              if (p.backdrop == BackgroundMode.transparent)
                Positioned.fill(
                  child: IgnorePointer(
                    child: AnimatedContainer(
                      key: const ValueKey('transparent-canvas-tint'),
                      duration: motionDuration(context, 280),
                      color: (p.surfaces.canvasColor ?? p.surface).withValues(
                        alpha: p.surfaces.canvasOpacity,
                      ),
                    ),
                  ),
                ),
              if (p.liquidCanvas)
                Positioned.fill(
                  child: IgnorePointer(
                    child: LiquidGlassSurface(
                      key: const ValueKey('liquid-canvas'),
                      canvas: true,
                      transparentCanvas:
                          p.backdrop == BackgroundMode.transparent,
                      tint: p.surface,
                      dark: p.dark,
                      borderRadius: BorderRadius.circular(p.windowRadius),
                      child: const SizedBox.expand(),
                    ),
                  ),
                ),
              SafeArea(
                minimum: EdgeInsets.only(top: widget.desktopCaption ? 32 : 0),
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final desktop = constraints.maxWidth >= 1050;
                    final sidebar = constraints.maxWidth >= 760;
                    return SettingsPageTransition(
                      showSettings: !desktop && compactSettingsOpen,
                      duration: desktop
                          ? Duration.zero
                          : motionDuration(context, 320),
                      settings: compactSettings(),
                      content: Padding(
                        padding: EdgeInsets.all(desktop ? 24 : 12),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            if (sidebar)
                              CollapsiblePanel(
                                key: const ValueKey('sidebar-panel'),
                                expanded: sidebarExpanded,
                                axis: Axis.horizontal,
                                extent: desktop ? 234 : 192,
                                child: Padding(
                                  padding: EdgeInsets.only(
                                    right: desktop ? 26 : 16,
                                  ),
                                  child: navigation(),
                                ),
                              ),
                            Expanded(
                              child: Column(
                                children: [
                                  header(sidebar, desktop),
                                  const SizedBox(height: 22),
                                  Expanded(
                                    child: Row(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                        Expanded(
                                          child: AnimatedSwitcher(
                                            duration:
                                                MediaQuery.disableAnimationsOf(
                                                  context,
                                                )
                                                ? Duration.zero
                                                : const Duration(
                                                    milliseconds: 300,
                                                  ),
                                            switchInCurve: Curves.easeOutCubic,
                                            switchOutCurve: Curves.easeInCubic,
                                            layoutBuilder:
                                                (current, previous) => Stack(
                                                  alignment:
                                                      Alignment.topCenter,
                                                  children: [
                                                    ...previous.map(
                                                      (child) => IgnorePointer(
                                                        child: ExcludeSemantics(
                                                          child: child,
                                                        ),
                                                      ),
                                                    ),
                                                    ?current,
                                                  ],
                                                ),
                                            transitionBuilder:
                                                (child, animation) =>
                                                    FadeTransition(
                                                      opacity: animation,
                                                      child: SlideTransition(
                                                        position: Tween<Offset>(
                                                          begin: const Offset(
                                                            0,
                                                            .025,
                                                          ),
                                                          end: Offset.zero,
                                                        ).animate(animation),
                                                        child: child,
                                                      ),
                                                    ),
                                            child: SingleChildScrollView(
                                              key: ValueKey('page-$section'),
                                              child: Column(
                                                crossAxisAlignment:
                                                    CrossAxisAlignment.start,
                                                children: [
                                                  greeting(),
                                                  const SizedBox(height: 24),
                                                  if (section == '概览')
                                                    hero()
                                                  else
                                                    pageIntro(),
                                                  const SizedBox(height: 28),
                                                  collectionHeader(),
                                                  const SizedBox(height: 6),
                                                  Align(
                                                    alignment:
                                                        Alignment.centerRight,
                                                    child: PopupMenuButton<String>(
                                                      tooltip: '排列想法',
                                                      initialValue: sort,
                                                      onSelected: (value) =>
                                                          setState(
                                                            () => sort = value,
                                                          ),
                                                      itemBuilder: (_) =>
                                                          [
                                                                '最近添加',
                                                                '收藏优先',
                                                                '标题排序',
                                                              ]
                                                              .map(
                                                                (
                                                                  label,
                                                                ) => PopupMenuItem(
                                                                  value: label,
                                                                  child: Text(
                                                                    label,
                                                                  ),
                                                                ),
                                                              )
                                                              .toList(),
                                                      child: Padding(
                                                        padding:
                                                            const EdgeInsets.all(
                                                              6,
                                                            ),
                                                        child: Row(
                                                          mainAxisSize:
                                                              MainAxisSize.min,
                                                          children: [
                                                            Icon(
                                                              Icons
                                                                  .sort_rounded,
                                                              size: 13,
                                                              color: p.muted,
                                                            ),
                                                            const SizedBox(
                                                              width: 5,
                                                            ),
                                                            Text(
                                                              sort,
                                                              style: TextStyle(
                                                                fontSize: 10,
                                                                color: p.muted,
                                                              ),
                                                            ),
                                                          ],
                                                        ),
                                                      ),
                                                    ),
                                                  ),
                                                  const SizedBox(height: 16),
                                                  SoftSize(
                                                    duration: motionDuration(
                                                      context,
                                                      240,
                                                    ),
                                                    alignment:
                                                        Alignment.topCenter,
                                                    child: AnimatedSwitcher(
                                                      key: const ValueKey(
                                                        'cards-transition',
                                                      ),
                                                      duration: motionDuration(
                                                        context,
                                                        220,
                                                      ),
                                                      layoutBuilder:
                                                          (
                                                            current,
                                                            previous,
                                                          ) => Stack(
                                                            alignment: Alignment
                                                                .topCenter,
                                                            children: [
                                                              ...previous.map(
                                                                (
                                                                  child,
                                                                ) => IgnorePointer(
                                                                  child:
                                                                      ExcludeSemantics(
                                                                        child:
                                                                            child,
                                                                      ),
                                                                ),
                                                              ),
                                                              ?current,
                                                            ],
                                                          ),
                                                      child: KeyedSubtree(
                                                        key: ValueKey(
                                                          '$filter/$sort/$query/${visibleIdeas.map((idea) => idea.id).join(',')}',
                                                        ),
                                                        child: cards(),
                                                      ),
                                                    ),
                                                  ),
                                                  const SizedBox(height: 22),
                                                  quickCapture(),
                                                  const SizedBox(height: 22),
                                                  const SizedBox(height: 8),
                                                ],
                                              ),
                                            ),
                                          ),
                                        ),
                                        if (desktop)
                                          CollapsiblePanel(
                                            key: const ValueKey(
                                              'settings-side-panel',
                                            ),
                                            expanded: showAppearance,
                                            axis: Axis.horizontal,
                                            extent: 276,
                                            child: Padding(
                                              padding: const EdgeInsets.only(
                                                left: 24,
                                              ),
                                              child: SingleChildScrollView(
                                                child: Column(
                                                  children: [
                                                    appearance(),
                                                    const SizedBox(height: 20),
                                                    scratchpad(),
                                                    const SizedBox(height: 20),
                                                    musicPanel(),
                                                    const SizedBox(height: 20),
                                                    smallQuote(),
                                                  ],
                                                ),
                                              ),
                                            ),
                                          ),
                                      ],
                                    ),
                                  ),
                                  const SizedBox(height: 12),
                                  Glass(
                                    key: const ValueKey('footer-dock'),
                                    componentId: 'footer',
                                    p: p,
                                    radius: 14,
                                    child: Padding(
                                      padding: const EdgeInsets.symmetric(
                                        horizontal: 14,
                                        vertical: 4,
                                      ),
                                      child: MusicFooter(music: music),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget navigation() => Glass(
    componentId: 'navigation',
    p: p,
    radius: 26,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(16, 26, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(left: 12),
            child: Row(
              children: [
                logo(32),
                const SizedBox(width: 11),
                Expanded(
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    alignment: Alignment.centerLeft,
                    child: Text(
                      'Morrow',
                      style: TextStyle(
                        fontSize: 23,
                        color: p.ink,
                        fontWeight: FontWeight.w600,
                        letterSpacing: -1,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 9),
          Padding(
            padding: const EdgeInsets.only(left: 13),
            child: Text(
              '杂事有序，奇想自由。',
              style: TextStyle(fontSize: 10, color: p.muted, letterSpacing: 1),
            ),
          ),
          const SizedBox(height: 42),
          Padding(
            padding: const EdgeInsets.only(left: 13, bottom: 14),
            child: Text(
              '我的空间',
              style: TextStyle(
                fontSize: 10,
                color: p.muted,
                letterSpacing: 1.5,
              ),
            ),
          ),
          navItem('概览', Icons.grid_view_rounded),
          navItem(
            '灵感收件箱',
            Icons.inbox_outlined,
            count: ideas.where((e) => e.category == '灵感').length,
          ),
          navItem('小项目', Icons.folder_open_rounded),
          navItem('实验室', Icons.science_outlined),
          const SizedBox(height: 14),
          Divider(color: p.line, indent: 12, endIndent: 12),
          const SizedBox(height: 14),
          navItem('已收藏', Icons.bookmark_border_rounded),
          const Spacer(),
          Container(
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: p.accent.withValues(alpha: .065),
              borderRadius: p.borderRadius(16),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(Icons.wb_twilight_rounded, color: p.accent, size: 23),
                const SizedBox(height: 9),
                RotatingTip(
                  key: const ValueKey('corner-tips'),
                  lines: cornerTips,
                  style: TextStyle(fontSize: 10, color: p.muted, height: 1.8),
                ),
              ],
            ),
          ),
          const SizedBox(height: 22),
          Row(
            children: [
              CircleAvatar(
                radius: 17,
                backgroundColor: p.accent.withValues(alpha: .13),
                child: Text(
                  'D',
                  style: TextStyle(color: p.accent, fontSize: 13),
                ),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('个人工作台', style: TextStyle(fontSize: 11, color: p.ink)),
                    Text(
                      'Just for your curiosity',
                      style: TextStyle(fontSize: 9, color: p.muted),
                    ),
                  ],
                ),
              ),
              Icon(Icons.more_horiz, color: p.muted, size: 17),
            ],
          ),
        ],
      ),
    ),
  );

  Widget logo(double size) => Container(
    width: size,
    height: size,
    decoration: BoxDecoration(
      color: p.accent,
      borderRadius: p.borderRadius(size * .34),
    ),
    child: Icon(Icons.all_inclusive_rounded, size: 23, color: p.onAccent),
  );

  Widget navItem(String title, IconData icon, {int? count}) {
    final selected = section == title;
    return Padding(
      padding: const EdgeInsets.only(bottom: 5),
      child: AnimatedContainer(
        duration: motionDuration(context, 220),
        decoration: BoxDecoration(
          color: selected
              ? p.accent.withValues(alpha: .13)
              : Colors.transparent,
          borderRadius: p.borderRadius(12),
        ),
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: p.borderRadius(12),
            onTap: () => selectSection(title),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 13),
              child: Row(
                children: [
                  Icon(icon, size: 18, color: selected ? p.accent : p.muted),
                  const SizedBox(width: 11),
                  Expanded(
                    child: Text(
                      title,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: selected
                            ? FontWeight.w600
                            : FontWeight.w400,
                        color: selected ? p.accent : p.muted,
                      ),
                    ),
                  ),
                  if (count != null)
                    Text(
                      '$count',
                      style: TextStyle(fontSize: 10, color: p.muted),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  void selectSection(String value) => setState(() {
    FocusManager.instance.primaryFocus?.unfocus();
    section = value;
    filter = '全部';
  });

  Widget header(bool sidebar, bool desktop) => Row(
    children: [
      if (!sidebar)
        PopupMenuButton<String>(
          tooltip: '导航',
          onSelected: selectSection,
          icon: Icon(Icons.menu_rounded, color: p.ink),
          itemBuilder: (_) => [
            '概览',
            '灵感收件箱',
            '小项目',
            '实验室',
            '已收藏',
          ].map((s) => PopupMenuItem(value: s, child: Text(s))).toList(),
        )
      else ...[
        IconButton(
          key: const ValueKey('sidebar-toggle'),
          tooltip: sidebarExpanded ? '收起侧边栏' : '展开侧边栏',
          onPressed: () {
            setState(() => sidebarExpanded = !sidebarExpanded);
            persist();
          },
          icon: AnimatedRotation(
            turns: sidebarExpanded ? 0 : .5,
            duration: motionDuration(context, 340),
            child: Icon(Icons.chevron_left_rounded, size: 19, color: p.muted),
          ),
        ),
        Icon(Icons.space_dashboard_outlined, size: 16, color: p.muted),
        const SizedBox(width: 9),
        Text('工作台', style: TextStyle(color: p.muted, fontSize: 11)),
        const SizedBox(width: 10),
        Text('/', style: TextStyle(color: p.muted)),
        const SizedBox(width: 10),
        Text(section, style: TextStyle(color: p.ink, fontSize: 11)),
        const Spacer(),
      ],
      if (sidebar)
        SizedBox(width: 246, child: searchField())
      else
        Expanded(child: searchField()),
      const SizedBox(width: 8),
      IconButton(
        key: const ValueKey('appearance-toggle'),
        tooltip: desktop && showAppearance ? '收起外观设置' : '显示外观设置',
        onPressed: () {
          if (desktop) {
            setState(() => showAppearance = !showAppearance);
            persist();
          } else {
            FocusManager.instance.primaryFocus?.unfocus();
            setState(() => compactSettingsOpen = true);
          }
        },
        icon: Icon(Icons.tune_rounded, size: 19, color: p.muted),
      ),
    ],
  );

  Widget searchField() => SizedBox(
    key: const ValueKey('header-search'),
    height: 38,
    child: Glass(
      componentId: 'search',
      p: p,
      radius: 12,
      child: TextField(
        controller: search,
        focusNode: searchFocus,
        style: TextStyle(fontSize: 11, color: p.ink),
        decoration: InputDecoration(
          prefixIcon: Icon(Icons.search, size: 17, color: p.muted),
          hintText: '搜索你的奇思妙想…',
          hintStyle: TextStyle(fontSize: 11, color: p.muted),
          border: InputBorder.none,
          contentPadding: const EdgeInsets.symmetric(vertical: 13),
          suffixIcon: search.text.isEmpty
              ? Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Text(
                    'Ctrl K',
                    style: TextStyle(fontSize: 9, color: p.muted),
                  ),
                )
              : IconButton(
                  tooltip: '清空搜索',
                  onPressed: () => setState(() {
                    search.clear();
                    query = '';
                  }),
                  icon: const Icon(Icons.close, size: 14),
                ),
        ),
      ),
    ),
  );

  Widget greeting() => Row(
    crossAxisAlignment: CrossAxisAlignment.center,
    children: [
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'A LITTLE SPACE FOR BIG IDEAS',
              style: TextStyle(fontSize: 9, color: p.accent, letterSpacing: 2),
            ),
            const SizedBox(height: 9),
            Text(
              section == '概览' ? '让想法，自由生长。' : section,
              style: TextStyle(
                fontSize: 27,
                height: 1.3,
                fontWeight: FontWeight.w600,
                letterSpacing: -.7,
                color: p.ink,
              ),
            ),
            const SizedBox(height: 8),
            Text(
              '收纳日常的零碎，也留住灵光一闪。',
              style: TextStyle(fontSize: 12, color: p.muted),
            ),
          ],
        ),
      ),
      const SizedBox(width: 12),
      FilledButton.icon(
        onPressed: createIdea,
        style: FilledButton.styleFrom(
          backgroundColor: p.accent,
          foregroundColor: p.onAccent,
          padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 17),
          shape: RoundedRectangleBorder(borderRadius: p.borderRadius(12)),
        ),
        icon: const Icon(Icons.add, size: 17),
        label: const Text('新建灵感', style: TextStyle(fontSize: 11)),
      ),
    ],
  );

  Widget hero() => LayoutBuilder(
    builder: (context, constraints) => Glass(
      componentId: 'hero',
      p: p,
      radius: 23,
      child: SizedBox(
        height: 214,
        child: Stack(
          children: [
            Positioned(
              right: constraints.maxWidth < 520 ? -145 : -20,
              top: -70,
              bottom: -70,
              width: 340,
              child: IgnorePointer(
                child: Opacity(
                  opacity: constraints.maxWidth < 520 ? .24 : 1,
                  child: CustomPaint(painter: OrbPainter(p)),
                ),
              ),
            ),
            Positioned.fill(
              child: Padding(
                padding: const EdgeInsets.all(25),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Container(
                          width: 5,
                          height: 5,
                          decoration: BoxDecoration(
                            color: p.accent,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 7),
                        Text(
                          'THE POSSIBILITY CORNER',
                          style: TextStyle(
                            fontSize: 8,
                            letterSpacing: 1.6,
                            color: p.accent,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 20),
                    Text(
                      '还没成形，也没关系。',
                      style: TextStyle(
                        fontSize: 21,
                        fontWeight: FontWeight.w500,
                        color: p.ink,
                      ),
                    ),
                    const SizedBox(height: 10),
                    Text(
                      '一个念头、一件小事、一个「万一呢」。\n这里是它们开始的地方。',
                      style: TextStyle(
                        fontSize: 11,
                        height: 1.85,
                        color: p.muted,
                      ),
                    ),
                    const Spacer(),
                    InkWell(
                      onTap: createIdea,
                      borderRadius: p.borderRadius(8),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(vertical: 5),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              '记录此刻的想法',
                              style: TextStyle(
                                fontSize: 11,
                                color: p.accent,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            const SizedBox(width: 10),
                            Icon(
                              Icons.arrow_forward_rounded,
                              color: p.accent,
                              size: 15,
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );

  Widget collectionHeader() => Wrap(
    alignment: WrapAlignment.spaceBetween,
    crossAxisAlignment: WrapCrossAlignment.center,
    spacing: 12,
    runSpacing: 12,
    children: [
      Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            section == '概览' ? '最近的念头' : section,
            style: TextStyle(
              fontSize: 15,
              color: p.ink,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            (widget.workbench?.writable ?? false) &&
                    _queries.phase != QueryPhase.ready
                ? '…'
                : visibleIdeas.length.toString().padLeft(2, '0'),
            style: TextStyle(fontSize: 10, color: p.muted),
          ),
        ],
      ),
      Row(
        mainAxisSize: MainAxisSize.min,
        children: pageFilters
            .map(
              (label) => Padding(
                padding: const EdgeInsets.only(left: 4),
                child: InkWell(
                  borderRadius: p.borderRadius(8),
                  onTap: () => setState(() => filter = label),
                  child: AnimatedContainer(
                    duration: motionDuration(context, 200),
                    padding: const EdgeInsets.symmetric(
                      horizontal: 11,
                      vertical: 7,
                    ),
                    decoration: BoxDecoration(
                      color: filter == label
                          ? p.accent.withValues(alpha: .13)
                          : Colors.transparent,
                      borderRadius: p.borderRadius(8),
                    ),
                    child: Text(
                      label,
                      style: TextStyle(
                        fontSize: 10,
                        color: filter == label ? p.accent : p.muted,
                      ),
                    ),
                  ),
                ),
              ),
            )
            .toList(),
      ),
    ],
  );

  Widget cards() {
    final items = visibleIdeas;
    if ((widget.workbench?.writable ?? false) &&
        _queries.phase != QueryPhase.ready) {
      final failed = _queries.phase == QueryPhase.failed;
      return Glass(
        componentId: 'query-status',
        p: p,
        child: SizedBox(
          height: 160,
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (!failed)
                  SizedBox(
                    width: 100,
                    child: LinearProgressIndicator(color: p.accent),
                  ),
                const SizedBox(height: 12),
                Text(
                  failed
                      ? (_queries.failure?.terminal ?? false)
                            ? '此次筛选已终止'
                            : '尚未确认筛选结果'
                      : '正在筛选…',
                  key: ValueKey(failed ? 'query-error' : 'query-loading'),
                  style: TextStyle(color: p.muted),
                ),
                if (failed)
                  TextButton(
                    key: const ValueKey('query-retry'),
                    onPressed: _queries.retry,
                    child: Text(
                      (_queries.failure?.terminal ?? false) ? '重新筛选' : '重试筛选',
                    ),
                  ),
              ],
            ),
          ),
        ),
      );
    }
    if (items.isEmpty) {
      return Glass(
        componentId: 'empty',
        p: p,
        child: SizedBox(
          height: 160,
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.search_off_rounded, color: p.muted),
                const SizedBox(height: 12),
                Text('这里还没有匹配的想法', style: TextStyle(color: p.muted)),
                TextButton(
                  onPressed: () => setState(() {
                    search.clear();
                    query = '';
                    filter = '全部';
                    section = '概览';
                  }),
                  child: const Text('查看全部'),
                ),
              ],
            ),
          ),
        ),
      );
    }
    if (section != '概览') return specializedCards(items);
    return LayoutBuilder(
      builder: (_, constraints) {
        final columns = constraints.maxWidth >= 460 ? 2 : 1;
        return Wrap(
          spacing: 14,
          runSpacing: 14,
          children: items
              .map(
                (idea) => SizedBox(
                  width: (constraints.maxWidth - 14 * (columns - 1)) / columns,
                  child: ideaCard(idea),
                ),
              )
              .toList(),
        );
      },
    );
  }

  Widget ideaCard(Idea idea) => Glass(
    componentId: 'card:${idea.id}',
    p: p,
    radius: 19,
    child: Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: () => openIdea(idea),
        borderRadius: p.borderRadius(19),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(19, 16, 13, 17),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Container(
                    width: 34,
                    height: 34,
                    decoration: BoxDecoration(
                      color: p
                          .componentColor(idea.color)
                          .withValues(alpha: .15),
                      borderRadius: p.borderRadius(11),
                    ),
                    child: Icon(
                      idea.icon,
                      color: p.dark
                          ? Color.lerp(
                              p.componentColor(idea.color),
                              Colors.white,
                              .3,
                            )
                          : p.componentColor(idea.color),
                      size: 19,
                    ),
                  ),
                  const Spacer(),
                  IconButton(
                    tooltip: idea.favorite
                        ? '取消收藏 ${idea.title}'
                        : '收藏 ${idea.title}',
                    constraints: const BoxConstraints(
                      minWidth: 32,
                      minHeight: 32,
                    ),
                    padding: EdgeInsets.zero,
                    onPressed: () {
                      if (widget.workbench != null) {
                        pluginChange(
                          PluginAction.favorite,
                          idea,
                          flag: !idea.favorite,
                        );
                        return;
                      }
                      setState(() => idea.favorite = !idea.favorite);
                      persist();
                    },
                    icon: SoftSwap(
                      child: Icon(
                        idea.favorite
                            ? Icons.bookmark_rounded
                            : Icons.bookmark_border_rounded,
                        key: ValueKey(idea.favorite),
                        size: 17,
                        color: idea.favorite
                            ? p.accent
                            : p.muted.withValues(alpha: .5),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 14),
              Text(
                idea.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                  color: p.ink,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                idea.description,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 11, color: p.muted, height: 1.8),
              ),
              if (idea.attachments.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: 10),
                  child: Text(
                    '附件 ${idea.attachments.length} · ${idea.attachments.first.source.name}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 10, color: p.accent),
                  ),
                ),
              const SizedBox(height: 19),
              Row(
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 7,
                      vertical: 4,
                    ),
                    decoration: BoxDecoration(
                      color: p.componentColor(idea.color).withValues(alpha: .1),
                      borderRadius: p.borderRadius(5),
                    ),
                    child: Text(
                      idea.category,
                      style: TextStyle(
                        fontSize: 9,
                        color: p.dark
                            ? Color.lerp(
                                p.componentColor(idea.color),
                                Colors.white,
                                .4,
                              )
                            : Color.lerp(
                                p.componentColor(idea.color),
                                Colors.black,
                                .16,
                              ),
                      ),
                    ),
                  ),
                  const Spacer(),
                  Text(
                    idea.time,
                    style: TextStyle(fontSize: 9, color: p.muted),
                  ),
                  const SizedBox(width: 7),
                ],
              ),
            ],
          ),
        ),
      ),
    ),
  );

  Widget quickCapture() => Glass(
    componentId: 'quick-capture',
    p: p,
    radius: 17,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 9, 8),
      child: Row(
        children: [
          Icon(Icons.edit_note_rounded, color: p.accent, size: 22),
          const SizedBox(width: 10),
          Expanded(
            child: TextField(
              controller: quickNote,
              onSubmitted: (_) => saveQuickNote(),
              style: TextStyle(fontSize: 12, color: p.ink),
              decoration: InputDecoration(
                hintText: '脑海中闪过了什么？',
                hintStyle: TextStyle(fontSize: 11, color: p.muted),
                border: InputBorder.none,
              ),
            ),
          ),
          IconButton(
            tooltip: '记录灵感',
            onPressed: saveQuickNote,
            icon: Icon(Icons.arrow_upward_rounded, size: 18, color: p.accent),
          ),
        ],
      ),
    ),
  );

  Future<void> saveQuickNote() async {
    final text = quickNote.text.trim();
    if (text.isEmpty) return;
    if (widget.workbench != null) {
      final result = await pluginChange(
        PluginAction.create,
        Idea(
          text,
          '从一个小小的念头开始。',
          section == '小项目'
              ? '进行中'
              : section == '实验室'
              ? '实验'
              : '灵感',
          Icons.auto_awesome_outlined,
          const Color(0xFF9D87D4),
          favorite: section == '已收藏',
        ),
      );
      if (result != null && mounted) {
        setState(() {
          quickNote.clear();
          filter = '全部';
          query = '';
          search.clear();
        });
      }
      return;
    }
    setState(() {
      ideas.insert(
        0,
        Idea(
          text,
          '从一个小小的念头开始。',
          section == '小项目'
              ? '进行中'
              : section == '实验室'
              ? '实验'
              : '灵感',
          Icons.auto_awesome_outlined,
          const Color(0xFF9D87D4),
          favorite: section == '已收藏',
        ),
      );
      quickNote.clear();
      filter = '全部';
      query = '';
      search.clear();
    });
    persist();
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('灵感已收好。'), duration: Duration(seconds: 2)),
    );
  }

  void updateSurfaces(SurfaceSettings value) => widget.onSurfaces?.call(value);

  Widget materialSlider(
    String key,
    String title,
    double value,
    double max,
    ValueChanged<double> onChanged, {
    bool enabled = true,
  }) => Column(
    children: [
      Row(
        children: [
          Text(title, style: TextStyle(fontSize: 10, color: p.muted)),
          const Spacer(),
          Text(
            '${(value / max * 100).round()}%',
            style: TextStyle(fontSize: 11, color: p.accent),
          ),
        ],
      ),
      SliderTheme(
        data: SliderTheme.of(context).copyWith(
          trackHeight: 3,
          thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
        ),
        child: Slider(
          key: ValueKey(key),
          value: value,
          min: 0,
          max: max,
          divisions: 100,
          onChanged: enabled ? onChanged : null,
          onChangeEnd: (_) => widget.onAppearanceCommit(),
        ),
      ),
    ],
  );

  Future<void> chooseSurfaceColor(bool canvas) async {
    final before = p.surfaces;
    final chosen = await showStudioDialog<Color>(
      context: context,
      builder: (_) => ColorCompassDialog(
        title: canvas ? '画布染色罗盘' : '组件染色罗盘',
        initial:
            (canvas ? before.canvasColor : before.componentColor) ?? p.surface,
        onChanged: (color) => updateSurfaces(
          canvas
              ? before.copyWith(canvasColor: color)
              : before.copyWith(componentColor: color),
        ),
      ),
    );
    if (!mounted) return;
    updateSurfaces(
      chosen == null
          ? before
          : canvas
          ? before.copyWith(canvasColor: chosen)
          : before.copyWith(componentColor: chosen),
    );
    if (chosen != null) widget.onAppearanceCommit();
  }

  Widget surfaceColorButton(bool canvas) => OutlinedButton.icon(
    key: ValueKey(canvas ? 'canvas-color' : 'component-color'),
    onPressed: () => chooseSurfaceColor(canvas),
    icon: Icon(Icons.palette_outlined, size: 16, color: p.accent),
    label: const Text('调色罗盘 · 自定义', style: TextStyle(fontSize: 11)),
    style: OutlinedButton.styleFrom(
      minimumSize: const Size.fromHeight(38),
      side: BorderSide(color: p.line),
      shape: RoundedRectangleBorder(borderRadius: p.borderRadius(11)),
    ),
  );

  Widget transparentMaterialSettings() => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      const SizedBox(height: 12),
      materialSlider(
        'canvas-blur',
        '磨砂效果',
        p.surfaces.canvasBlur,
        40,
        (value) => updateSurfaces(p.surfaces.copyWith(canvasBlur: value)),
        enabled: isWindowsDesktop,
      ),
      materialSlider(
        'canvas-opacity',
        '染色不透明度',
        p.surfaces.canvasOpacity,
        1,
        (value) => updateSurfaces(p.surfaces.copyWith(canvasOpacity: value)),
      ),
      surfaceColorButton(true),
      if (!isWindowsDesktop)
        Text(
          '桌面磨砂仅在 Windows 版可用',
          style: TextStyle(fontSize: 10, color: p.muted),
        ),
    ],
  );

  Widget componentMaterialSettings() => OutlinedButton.icon(
    key: const ValueKey('component-settings'),
    onPressed: () => Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => ComponentMaterialListPage(
          palette: p,
          entries: {
            'navigation': '侧边导航',
            'search': '搜索栏',
            'hero': '概览卡片',
            'quick-capture': '快速记录',
            'appearance': '空间外观',
            if (widget.workbench is WorkbenchPluginControl)
              'plugin-tools': '工作台插件',
            if (widget.workbench is WorkbenchProtectionBackup)
              'protection-backup': '内容保护',
            'daily': '此刻的小事',
            'music': '随身听',
            'footer': '底部提示与歌词',
            'empty': '空白提示',
            for (final name in ['灵感收件箱', '小项目', '实验室', '已收藏'])
              'summary:$name': '$name · 概览',
            for (final idea in ideas) 'card:${idea.id}': idea.title,
          },
          onChanged: (value) {
            if (!mounted) return;
            updateSurfaces(value);
            widget.onAppearanceCommit();
          },
        ),
      ),
    ),
    icon: Icon(Icons.tune_rounded, size: 16, color: p.accent),
    label: const Text('组件与卡片 · 独立设置', style: TextStyle(fontSize: 11)),
    style: OutlinedButton.styleFrom(
      minimumSize: const Size.fromHeight(38),
      side: BorderSide(color: p.line),
      shape: RoundedRectangleBorder(borderRadius: p.borderRadius(11)),
    ),
  );

  Widget appearance() {
    final Object? backend = widget.workbench;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        appearanceControls(),
        if (backend is WorkbenchPluginControl) ...[
          const SizedBox(height: 14),
          Glass(
            componentId: 'plugin-tools',
            p: p,
            radius: 22,
            child: PluginTools(
              backend: backend,
              onChanged: () {
                if (mounted) setState(() => _queries.invalidate());
              },
              ink: p.ink,
              muted: p.muted,
              line: p.line,
              radius: p.borderRadius(11),
            ),
          ),
        ],
        if (backend is WorkbenchProtectionBackup) ...[
          const SizedBox(height: 14),
          Glass(
            componentId: 'protection-backup',
            p: p,
            radius: 22,
            child: ProtectionBackup(
              onBackup: backend.backupProtection,
              onSnapshot: backend.backupSnapshot,
              ink: p.ink,
              muted: p.muted,
              line: p.line,
              radius: p.borderRadius(11),
            ),
          ),
        ],
      ],
    );
  }

  Widget appearanceControls() => Glass(
    componentId: 'appearance',
    p: p,
    radius: 22,
    child: Padding(
      padding: const EdgeInsets.all(19),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.tune_rounded, size: 16, color: p.ink),
              const SizedBox(width: 8),
              Text(
                '空间外观',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: p.ink,
                ),
              ),
              const Spacer(),
              Text(
                'MAKE IT YOURS',
                style: TextStyle(fontSize: 7, letterSpacing: 1, color: p.muted),
              ),
            ],
          ),
          const SizedBox(height: 23),
          label('玻璃质感'),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: modeOption(
                  GlassMode.frosted,
                  '磨砂',
                  Icons.blur_on_rounded,
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: modeOption(
                  GlassMode.clear,
                  '超透',
                  Icons.water_drop_outlined,
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: modeOption(GlassMode.liquid, '液体玻璃', Icons.lens_blur),
              ),
            ],
          ),
          SoftSize(
            duration: motionDuration(context, 220),
            alignment: Alignment.topCenter,
            child: Column(
              children: [
                if (p.mode == GlassMode.frosted) ...[
                  const SizedBox(height: 16),
                  Row(
                    children: [
                      Text(
                        '磨砂不透明度',
                        style: TextStyle(fontSize: 10, color: p.muted),
                      ),
                      const Spacer(),
                      Text(
                        '${(p.frostedOpacity * 100).round()}%',
                        style: TextStyle(fontSize: 11, color: p.accent),
                      ),
                    ],
                  ),
                  SliderTheme(
                    data: SliderTheme.of(context).copyWith(
                      trackHeight: 3,
                      thumbShape: const RoundSliderThumbShape(
                        enabledThumbRadius: 6,
                      ),
                      overlayShape: const RoundSliderOverlayShape(
                        overlayRadius: 12,
                      ),
                    ),
                    child: Slider(
                      key: const ValueKey('frosted-opacity'),
                      min: .2,
                      max: 1,
                      divisions: 80,
                      value: p.frostedOpacity,
                      label: '${(p.frostedOpacity * 100).round()}%',
                      onChanged: widget.onOpacity,
                      onChangeEnd: (_) => widget.onAppearanceCommit(),
                    ),
                  ),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(
                        '20% · 轻盈',
                        style: TextStyle(fontSize: 9, color: p.muted),
                      ),
                      Text(
                        '100% · 纯粹',
                        style: TextStyle(fontSize: 9, color: p.muted),
                      ),
                    ],
                  ),
                ],
              ],
            ),
          ),
          componentMaterialSettings(),
          const SizedBox(height: 14),
          SizedBox(
            height: 108,
            width: double.infinity,
            child: ClipRRect(
              borderRadius: p.borderRadius(14),
              child: Stack(
                children: [
                  Positioned.fill(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: LinearGradient(
                          begin: Alignment.topLeft,
                          end: Alignment.bottomRight,
                          colors: p.dark
                              ? [
                                  p.themeTint(const Color(0xFF424260), .6),
                                  p.themeTint(const Color(0xFF736381), .6),
                                  p.themeTint(const Color(0xFF343E45), .6),
                                ]
                              : [
                                  p.themeTint(const Color(0xFFD9E5DB), .6),
                                  p.themeTint(const Color(0xFFD6C3E5), .6),
                                  p.themeTint(const Color(0xFFEDDED6), .6),
                                ],
                        ),
                      ),
                    ),
                  ),
                  Positioned(
                    left: 20,
                    top: 13,
                    child: Container(
                      width: 58,
                      height: 58,
                      decoration: BoxDecoration(
                        color: p.themeTint(const Color(0xFFA18AC7), .6),
                        shape: BoxShape.circle,
                      ),
                    ),
                  ),
                  Positioned(
                    right: 24,
                    bottom: -10,
                    child: Container(
                      width: 70,
                      height: 70,
                      decoration: BoxDecoration(
                        color: p.themeTint(const Color(0xFFDBC1A6), .6),
                        shape: BoxShape.circle,
                      ),
                    ),
                  ),
                  Center(
                    child: SizedBox(
                      width: 140,
                      height: 68,
                      child: Glass(
                        p: p,
                        radius: 13,
                        child: Center(
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(
                                p.clear
                                    ? Icons.water_drop_outlined
                                    : Icons.blur_on_rounded,
                                color: p.ink,
                                size: 19,
                              ),
                              const SizedBox(height: 5),
                              Text(
                                p.liquid
                                    ? 'Liquid glass'
                                    : p.clear
                                    ? 'Crystal clear'
                                    : 'Softly frosted',
                                style: TextStyle(
                                  fontSize: 10,
                                  color: p.ink,
                                  letterSpacing: .4,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
          Text(
            p.liquid
                ? '流动的高光、柔和折射，让面板像一滴凝住的水。'
                : p.clear
                ? '通透轻盈，让光与色彩穿过界面。'
                : '柔化背景，让思绪安静地浮现。',
            style: TextStyle(fontSize: 9, color: p.muted),
          ),
          const SizedBox(height: 22),
          label('主题色调'),
          const SizedBox(height: 12),
          Row(
            children: StudioTheme.values
                .map((theme) => Expanded(child: themeOption(theme)))
                .toList(),
          ),
          const SizedBox(height: 12),
          OutlinedButton.icon(
            key: const ValueKey('theme-color-compass'),
            onPressed: chooseThemeColor,
            icon: Icon(Icons.palette_outlined, color: p.accent, size: 16),
            label: const Text('主题色彩色罗盘', style: TextStyle(fontSize: 11)),
            style: OutlinedButton.styleFrom(
              minimumSize: const Size.fromHeight(38),
              side: BorderSide(color: p.line),
              shape: RoundedRectangleBorder(borderRadius: p.borderRadius(11)),
            ),
          ),
          Row(
            children: [
              Expanded(
                child: Text(
                  p.themeColor == null
                      ? '默认主题色 · 全局控件'
                      : '#${p.themeColor!.toARGB32().toRadixString(16).substring(2).toUpperCase()} · 全局控件',
                  style: TextStyle(color: p.muted, fontSize: 10),
                ),
              ),
              TextButton(
                key: const ValueKey('theme-color-reset'),
                onPressed: p.themeColor == null
                    ? null
                    : () {
                        widget.onThemeColor(null);
                        widget.onAppearanceCommit();
                      },
                child: const Text('恢复默认'),
              ),
            ],
          ),
          if (p.isCustom) ...[
            const SizedBox(height: 12),
            TextButton(
              key: const ValueKey('custom-tone-toggle'),
              onPressed: () => setState(() => showCustomTone = !showCustomTone),
              child: Row(
                children: [
                  Expanded(child: Text(showCustomTone ? '收起自定义色调' : '调整自定义色调')),
                  AnimatedRotation(
                    turns: showCustomTone ? .5 : 0,
                    duration: motionDuration(context, 200),
                    child: const Icon(Icons.expand_more, size: 18),
                  ),
                ],
              ),
            ),
            SoftSize(
              duration: motionDuration(context, 220),
              child: showCustomTone
                  ? Column(
                      children: [
                        Row(
                          children: [
                            Expanded(child: label('主题灰度')),
                            label('${(p.grayscale * 100).round()}%'),
                          ],
                        ),
                        Slider(
                          key: const ValueKey('theme-grayscale'),
                          value: p.grayscale,
                          divisions: 100,
                          label: '${(p.grayscale * 100).round()}%',
                          onChanged: widget.onGrayscale,
                          onChangeEnd: (_) => widget.onAppearanceCommit(),
                        ),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [label('保留原色'), label('黑白灰')],
                        ),
                        Divider(height: 28, color: p.line),
                        Row(
                          children: [
                            Expanded(child: label('自定义明暗')),
                            label('${(p.lightness * 100).round()}%'),
                          ],
                        ),
                        Slider(
                          key: const ValueKey('theme-lightness'),
                          value: p.lightness,
                          divisions: 100,
                          label: '${(p.lightness * 100).round()}%',
                          onChanged: widget.onLightness,
                          onChangeEnd: (_) => widget.onAppearanceCommit(),
                        ),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [label('深黑'), label('亮白')],
                        ),
                      ],
                    )
                  : const SizedBox.shrink(),
            ),
          ],
          Divider(height: 28, color: p.line),
          Row(
            children: [
              Expanded(child: label('圆角幅度')),
              label('${p.cornerRadius.round()} / 32'),
            ],
          ),
          Slider(
            key: const ValueKey('corner-radius'),
            value: p.cornerRadius,
            min: 0,
            max: 32,
            divisions: 32,
            label: '${p.cornerRadius.round()}',
            onChanged: widget.onRadius,
            onChangeEnd: (_) => widget.onAppearanceCommit(),
          ),
          Text('拖至 0 即为方角', style: TextStyle(fontSize: 9, color: p.muted)),
          if (widget.desktopCaption) ...[
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(child: label('窗体圆角')),
                label('${p.windowRadius.round()} / 32'),
              ],
            ),
            Slider(
              key: const ValueKey('window-radius'),
              value: p.windowRadius,
              max: 32,
              divisions: 32,
              label: '${p.windowRadius.round()}',
              onChanged: widget.onWindowRadius,
              onChangeEnd: (_) => widget.onAppearanceCommit(),
            ),
            Text(
              '独立调整窗口边框，最大化时自动展平',
              style: TextStyle(fontSize: 9, color: p.muted),
            ),
          ],
          Divider(height: 24, color: p.line),
          label('背景画布'),
          const SizedBox(height: 10),
          LayoutBuilder(
            builder: (_, constraints) => Wrap(
              spacing: 8,
              runSpacing: 8,
              children: BackgroundMode.values
                  .map(
                    (mode) => SizedBox(
                      width: (constraints.maxWidth - 8) / 2,
                      child: backgroundOption(mode),
                    ),
                  )
                  .toList(),
            ),
          ),
          SwitchListTile.adaptive(
            key: const ValueKey('canvas-liquid-toggle'),
            contentPadding: EdgeInsets.zero,
            title: const Text('液体玻璃效果', style: TextStyle(fontSize: 12)),
            subtitle: const Text(
              '独立于背景类型，四种画布均可开启',
              style: TextStyle(fontSize: 10),
            ),
            value: p.liquidCanvas,
            onChanged: widget.onLiquidCanvas,
          ),
          SoftSize(
            duration: motionDuration(context, 240),
            alignment: Alignment.topCenter,
            child: AnimatedSwitcher(
              duration: motionDuration(context, 220),
              child: Column(
                key: ValueKey(p.backdrop),
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (p.backdrop == BackgroundMode.transparent)
                    transparentMaterialSettings(),
                  if (p.backdrop == BackgroundMode.solid) ...[
                    const SizedBox(height: 12),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.spaceAround,
                      children: List.generate(
                        4,
                        (index) => Tooltip(
                          message: ['跟随主题', '淡紫', '鼠尾草', '暖沙'][index],
                          child: InkWell(
                            key: ValueKey('tint-$index'),
                            onTap: () => widget.onTint(index),
                            borderRadius: p.borderRadius(20),
                            child: Container(
                              width: 28,
                              height: 28,
                              decoration: BoxDecoration(
                                color: Palette(
                                  p.theme,
                                  p.mode,
                                  p.backdrop,
                                  index,
                                ).solidColor,
                                shape: BoxShape.circle,
                                border: Border.all(
                                  color:
                                      p.customColor == null &&
                                          p.solidTint == index
                                      ? p.accent
                                      : p.line,
                                  width: 1.5,
                                ),
                              ),
                              child:
                                  p.customColor == null && p.solidTint == index
                                  ? Icon(Icons.check, size: 13, color: p.ink)
                                  : null,
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 12),
                    OutlinedButton.icon(
                      key: const ValueKey('custom-color'),
                      onPressed: chooseColor,
                      icon: Icon(
                        Icons.palette_outlined,
                        size: 16,
                        color: p.accent,
                      ),
                      label: Text(
                        p.customColor == null
                            ? '调色罗盘 · 自定义'
                            : '#${p.customColor!.toARGB32().toRadixString(16).substring(2).toUpperCase()}',
                        style: const TextStyle(fontSize: 11),
                      ),
                      style: OutlinedButton.styleFrom(
                        minimumSize: const Size.fromHeight(38),
                        side: BorderSide(color: p.line),
                        shape: RoundedRectangleBorder(
                          borderRadius: p.borderRadius(11),
                        ),
                      ),
                    ),
                  ],
                  if (p.backdrop == BackgroundMode.texture) ...[
                    const SizedBox(height: 14),
                    Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        OutlinedButton.icon(
                          key: const ValueKey('texture-file'),
                          onPressed: importing ? null : () => chooseTexture(),
                          icon: const Icon(
                            Icons.upload_file_outlined,
                            size: 15,
                          ),
                          label: const Text(
                            '本地素材',
                            style: TextStyle(fontSize: 10),
                          ),
                        ),
                        OutlinedButton.icon(
                          key: const ValueKey('texture-link'),
                          onPressed: importing
                              ? null
                              : () => chooseTexture(online: true),
                          icon: const Icon(Icons.link, size: 15),
                          label: const Text(
                            '网络采集',
                            style: TextStyle(fontSize: 10),
                          ),
                        ),
                      ],
                    ),
                    if (importing)
                      const Padding(
                        padding: EdgeInsets.symmetric(vertical: 8),
                        child: LinearProgressIndicator(minHeight: 2),
                      ),
                    if (p.texture != null) ...[
                      const SizedBox(height: 10),
                      if (p.texture!.kind == TextureKind.video)
                        Row(
                          children: [
                            Expanded(child: label('播放背景声音')),
                            Switch(
                              key: const ValueKey('background-audio'),
                              value: backgroundSound,
                              onChanged: setBackgroundSound,
                            ),
                          ],
                        ),
                      Text(
                        p.texture!.name,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 10, color: p.ink),
                      ),
                      Row(
                        children: [
                          if (p.texture!.kind != TextureKind.image)
                            TextButton.icon(
                              key: const ValueKey('media-play'),
                              onPressed: () =>
                                  widget.onPlaying(!p.mediaPlaying),
                              icon: SoftSwap(
                                child: Icon(
                                  p.mediaPlaying
                                      ? Icons.pause_rounded
                                      : Icons.play_arrow_rounded,
                                  key: ValueKey(p.mediaPlaying),
                                  size: 16,
                                ),
                              ),
                              label: Text(
                                p.mediaPlaying ? '暂停' : '播放',
                                style: const TextStyle(fontSize: 10),
                              ),
                            ),
                          Expanded(
                            child: TextButton(
                              onPressed: () {
                                setState(() => mediaError = null);
                                widget.onTexture(null);
                              },
                              child: const Text(
                                '使用内置纹理',
                                style: TextStyle(fontSize: 10),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ] else
                      Text(
                        '图片 / GIF ≤ 25 MB，视频 ≤ 150 MB',
                        style: TextStyle(fontSize: 9, color: p.muted),
                      ),
                    if (mediaError != null)
                      Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: Text(
                          mediaError!,
                          style: TextStyle(
                            fontSize: 10,
                            color: Theme.of(context).colorScheme.error,
                            height: 1.6,
                          ),
                        ),
                      ),
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(height: 10),
          Text(switch (p.backdrop) {
            BackgroundMode.ambient => '流动的光晕，为灵感留一点色彩。',
            BackgroundMode.solid => '一张安静的纯色画布。',
            BackgroundMode.texture => '细密的纸感网点，让空间多一点触感。',
            BackgroundMode.transparent =>
              !kIsWeb && defaultTargetPlatform == TargetPlatform.android
                  ? '保留通透面板，以当前主题作为底色。'
                  : '透出窗口背后的空间；网页透出宿主背景。',
          }, style: TextStyle(fontSize: 9, color: p.muted, height: 1.6)),
          const SizedBox(height: 13),
          Row(
            children: [
              Icon(
                Icons.check_circle_outline_rounded,
                size: 12,
                color: p.accent,
              ),
              const SizedBox(width: 6),
              Text(
                '外观与灵感自动保存在本机',
                style: TextStyle(fontSize: 9, color: p.muted),
              ),
            ],
          ),
        ],
      ),
    ),
  );

  Widget label(String text) =>
      Text(text, style: TextStyle(fontSize: 10, color: p.muted));

  Widget backgroundOption(BackgroundMode mode) {
    final (title, icon) = switch (mode) {
      BackgroundMode.ambient => ('默认', Icons.gradient_rounded),
      BackgroundMode.solid => ('纯色', Icons.circle_outlined),
      BackgroundMode.texture => ('纹理', Icons.grain_rounded),
      BackgroundMode.transparent => ('透明', Icons.layers_clear_outlined),
    };
    final selected = p.backdrop == mode;
    return Semantics(
      button: true,
      selected: selected,
      child: InkWell(
        key: ValueKey('background-${mode.name}'),
        onTap: () => widget.onBackground(mode),
        borderRadius: p.borderRadius(10),
        child: AnimatedContainer(
          duration: motionDuration(context, 200),
          height: 38,
          decoration: BoxDecoration(
            color: selected
                ? p.accent.withValues(alpha: .14)
                : Colors.transparent,
            borderRadius: p.borderRadius(10),
            border: Border.all(
              color: selected ? p.accent.withValues(alpha: .5) : p.line,
            ),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, size: 14, color: selected ? p.accent : p.muted),
              const SizedBox(width: 7),
              Text(
                title,
                style: TextStyle(
                  fontSize: 10,
                  color: selected ? p.accent : p.muted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget modeOption(GlassMode mode, String title, IconData icon) {
    final selected = p.mode == mode;
    return Semantics(
      selected: selected,
      button: true,
      child: InkWell(
        key: ValueKey('mode-${mode.name}'),
        onTap: () => widget.onMode(mode),
        borderRadius: p.borderRadius(10),
        child: AnimatedContainer(
          duration: motionDuration(context, 220),
          height: 56,
          decoration: BoxDecoration(
            color: selected
                ? p.accent.withValues(alpha: .14)
                : p.surface.withValues(alpha: .12),
            border: Border.all(
              color: selected ? p.accent.withValues(alpha: .5) : p.line,
            ),
            borderRadius: p.borderRadius(10),
          ),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, size: 15, color: selected ? p.accent : p.muted),
              const SizedBox(height: 5),
              Text(
                title,
                style: TextStyle(
                  fontSize: 11,
                  color: selected ? p.accent : p.muted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget themeOption(StudioTheme theme) {
    final selected = p.theme == theme;
    final color = switch (theme) {
      StudioTheme.white => const Color(0xFFFAFAFC),
      StudioTheme.custom => const Color(0xFFCFD1DA),
      StudioTheme.dark => const Color(0xFF302E3E),
    };
    final title = switch (theme) {
      StudioTheme.white => '白色',
      StudioTheme.custom => '自定义',
      StudioTheme.dark => '深色',
    };
    return Semantics(
      selected: selected,
      button: true,
      child: InkWell(
        key: ValueKey('theme-${theme.name}'),
        onTap: () {
          setState(() => showCustomTone = false);
          widget.onTheme(theme);
        },
        borderRadius: p.borderRadius(10),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: Column(
            children: [
              AnimatedContainer(
                duration: motionDuration(context, 220),
                width: 40,
                height: 40,
                padding: const EdgeInsets.all(3),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(
                    color: selected ? p.accent : Colors.transparent,
                    width: 1.5,
                  ),
                ),
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    color: color,
                    shape: BoxShape.circle,
                    border: Border.all(color: const Color(0x22888899)),
                  ),
                  child: SoftSwap(
                    child: selected
                        ? Icon(
                            Icons.check,
                            key: const ValueKey('checked'),
                            size: 15,
                            color: theme == StudioTheme.dark
                                ? Colors.white
                                : const Color(0xFF746095),
                          )
                        : const SizedBox(
                            key: ValueKey('unchecked'),
                            width: 15,
                            height: 15,
                          ),
                  ),
                ),
              ),
              const SizedBox(height: 7),
              Text(
                title,
                style: TextStyle(
                  fontSize: 10,
                  color: selected ? p.accent : p.muted,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget scratchpad() => Glass(
    componentId: 'daily',
    p: p,
    radius: 22,
    child: Padding(
      padding: const EdgeInsets.all(19),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.bolt_outlined, color: p.accent, size: 17),
              const SizedBox(width: 7),
              Text(
                '此刻的小事',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: p.ink,
                ),
              ),
            ],
          ),
          const SizedBox(height: 17),
          LittleTask(
            title: '给自己倒一杯水',
            done: completed.contains('给自己倒一杯水'),
            onChanged: (done) {
              setState(() {
                if (done) {
                  completed.add('给自己倒一杯水');
                } else {
                  completed.remove('给自己倒一杯水');
                }
              });
              persist();
            },
          ),
          LittleTask(
            title: '把一个想法写下来',
            done: completed.contains('把一个想法写下来'),
            onChanged: (done) {
              setState(() {
                if (done) {
                  completed.add('把一个想法写下来');
                } else {
                  completed.remove('把一个想法写下来');
                }
              });
              persist();
            },
          ),
          LittleTask(
            title: '留十分钟，随便探索',
            done: completed.contains('留十分钟，随便探索'),
            onChanged: (done) {
              setState(() {
                if (done) {
                  completed.add('留十分钟，随便探索');
                } else {
                  completed.remove('留十分钟，随便探索');
                }
              });
              persist();
            },
          ),
          const SizedBox(height: 9),
          Text('慢一点，也是在向前。', style: TextStyle(fontSize: 9, color: p.muted)),
        ],
      ),
    ),
  );

  Widget smallQuote() => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 17, vertical: 8),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '“',
          style: TextStyle(
            color: p.accent.withValues(alpha: .4),
            fontSize: 42,
            height: 1,
          ),
        ),
        Text(
          '所有有趣的东西，\n都始于一点点好奇。',
          style: TextStyle(color: p.muted, fontSize: 12, height: 1.9),
        ),
        const SizedBox(height: 12),
        Text(
          'STAY CURIOUS. STAY YOU.',
          style: TextStyle(fontSize: 7, color: p.muted, letterSpacing: 1.4),
        ),
      ],
    ),
  );

  void _acceptEditorResult(Idea result) {
    setState(() {
      final index = ideas.indexWhere((idea) => idea.id == result.id);
      if (index < 0) {
        ideas.insert(0, result);
      } else {
        ideas[index] = result;
      }
      _contentGeneration++;
    });
    persist();
  }

  Future<WorkbenchEditorSession?> _openEditor(String id, bool create) async {
    final backend = widget.workbench;
    if (backend case WorkbenchEditorSupport editor) {
      return editor.openEditor(id, create: create);
    }
    return null;
  }

  void _editorOpenError(Object error) {
    if (mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text('编辑器暂时无法打开，请检查内容服务后重试。')));
    }
  }

  Future<void> _refreshAfterEditorClose() async {
    if (widget.workbench case WorkbenchEditorSupport editor) {
      try {
        final current = await editor.refreshEditorContent();
        if (!mounted) return;
        setState(() {
          ideas
            ..clear()
            ..addAll(current);
          _contentGeneration++;
        });
        persist();
      } catch (_) {
        if (mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('编辑器已关闭，但保存状态暂时无法确认。请重新打开工作台检查，避免重复创建。'),
            ),
          );
        }
      }
    }
  }

  Future<void> createIdea() async {
    final target = Idea.nextId();
    WorkbenchEditorSession? editor;
    try {
      editor = await _openEditor(target, true);
    } catch (error) {
      _editorOpenError(error);
      return;
    }
    if (!mounted) {
      await editor?.close();
      return;
    }
    final result = await showStudioDialog<Idea>(
      context: context,
      builder: (_) => NewIdeaDialog(
        targetId: target,
        editor: editor,
        initialFavorite: section == '已收藏',
        plugin: widget.workbench?.studio,
        initialCategory: switch (section) {
          '小项目' => '进行中',
          '实验室' => '实验',
          _ => '灵感',
        },
      ),
    );
    if (!mounted) return;
    if (result == null) {
      if (editor != null) await _refreshAfterEditorClose();
      return;
    }
    if (editor != null) {
      _acceptEditorResult(result);
      setState(() {
        filter = '全部';
        query = '';
        search.clear();
      });
      return;
    }
    if (widget.workbench != null) {
      if (section == '已收藏') result.favorite = true;
      final saved = await pluginChange(PluginAction.create, result);
      if (saved != null && mounted) {
        setState(() {
          filter = '全部';
          query = '';
          search.clear();
        });
      }
      return;
    }
    setState(() {
      ideas.insert(0, result);
      if (section == '已收藏') result.favorite = true;
      filter = '全部';
      query = '';
      search.clear();
    });
    persist();
  }

  Future<void> openIdea(Idea idea) async {
    final action = await showStudioDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (_, refresh) => StudioDialog(
          icon: idea.icon,
          title: idea.title,
          subtitle: '留住细节，让下一步更清楚。',
          content: SizedBox(
            width: 390,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    idea.category,
                    style: TextStyle(color: p.accent, fontSize: 12),
                  ),
                  const SizedBox(height: 18),
                  IdeaMarkdown(
                    data: idea.description,
                    attachments: idea.attachments,
                  ),
                  if (idea.attachments.isNotEmpty) ...[
                    const SizedBox(height: 18),
                    Text(
                      '附件 · ${idea.attachments.length}',
                      style: TextStyle(color: p.accent),
                    ),
                    ...idea.attachments.map(
                      (item) => AttachmentTile(attachment: item),
                    ),
                  ],
                  if (idea.category == '实验') ...[
                    const SizedBox(height: 18),
                    Text('假设', style: TextStyle(color: p.accent)),
                    SelectableText(
                      idea.hypothesis.isEmpty ? '还没有写下假设' : idea.hypothesis,
                    ),
                    const SizedBox(height: 12),
                    Text('观察与结论', style: TextStyle(color: p.accent)),
                    SelectableText(
                      idea.conclusion.isEmpty ? '等待一次新的发现' : idea.conclusion,
                    ),
                  ],
                  if (idea.todos.isNotEmpty) ...[
                    const SizedBox(height: 22),
                    Text(
                      '小小的进展 · ${idea.completed.length}/${idea.todos.length}',
                      style: TextStyle(color: p.accent, fontSize: 11),
                    ),
                    const SizedBox(height: 8),
                    ...idea.todos.map(
                      (todo) => LittleTask(
                        title: todo,
                        done: idea.completed.contains(todo),
                        onChanged: (done) async {
                          if (widget.workbench != null) {
                            final updated = await pluginChange(
                              PluginAction.todo,
                              idea,
                              text: todo,
                              flag: done,
                            );
                            if (updated != null && dialogContext.mounted) {
                              refresh(() => idea = updated);
                            }
                            return;
                          }
                          setState(() {
                            if (done) {
                              idea.completed.add(todo);
                            } else {
                              idea.completed.remove(todo);
                            }
                          });
                          refresh(() {});
                          persist();
                        },
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, 'delete'),
              child: Text(
                '删除',
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, 'edit'),
              child: const Text('编辑'),
            ),
            FilledButton.tonal(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('收好'),
            ),
          ],
        ),
      ),
    );
    if (!mounted) return;
    if (action == 'edit') {
      WorkbenchEditorSession? editor;
      try {
        editor = await _openEditor(idea.id, false);
      } catch (error) {
        _editorOpenError(error);
        return;
      }
      if (!mounted) {
        await editor?.close();
        return;
      }
      final edited = await showStudioDialog<Idea>(
        context: context,
        builder: (_) => NewIdeaDialog(
          initialIdea: idea,
          editor: editor,
          plugin: widget.workbench?.studio,
        ),
      );
      if (!mounted) return;
      if (edited == null) {
        if (editor != null) await _refreshAfterEditorClose();
        return;
      }
      if (editor != null) {
        _acceptEditorResult(edited);
        return;
      }
      if (widget.workbench != null) {
        await pluginChange(PluginAction.edit, edited);
        return;
      }
      setState(() {
        final index = ideas.indexWhere((item) => item.id == idea.id);
        if (index >= 0) ideas[index] = edited;
      });
      persist();
    } else if (action == 'delete') {
      final index = ideas.indexOf(idea);
      if (widget.workbench != null) {
        if (await pluginChange(PluginAction.delete, idea) == null || !mounted) {
          return;
        }
      } else {
        setState(() => ideas.remove(idea));
        persist();
      }
      ScaffoldMessenger.of(context).hideCurrentSnackBar();
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('已删除「${idea.title}」'),
          duration: const Duration(seconds: 8),
          action: SnackBarAction(
            label: '撤销',
            onPressed: () {
              if (!mounted || ideas.any((item) => item.id == idea.id)) return;
              if (widget.workbench != null) {
                pluginChange(PluginAction.restore, idea, position: index);
                return;
              }
              setState(() => ideas.insert(index.clamp(0, ideas.length), idea));
              persist();
            },
          ),
        ),
      );
    }
  }
}

class LittleTask extends StatelessWidget {
  const LittleTask({
    super.key,
    required this.title,
    required this.done,
    required this.onChanged,
  });
  final String title;
  final bool done;
  final ValueChanged<bool> onChanged;
  @override
  Widget build(BuildContext context) => InkWell(
    borderRadius: AppearanceScope.of(context).borderRadius(7),
    onTap: () => onChanged(!done),
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 9),
      child: Row(
        children: [
          SoftSwap(
            child: Icon(
              done ? Icons.check_circle_rounded : Icons.circle_outlined,
              key: ValueKey(done),
              color: Theme.of(
                context,
              ).colorScheme.primary.withValues(alpha: done ? .9 : .4),
              size: 16,
            ),
          ),
          const SizedBox(width: 9),
          Expanded(
            child: Text(
              title,
              style: TextStyle(
                fontSize: 11,
                color: Theme.of(context).textTheme.bodySmall?.color,
                decoration: done ? TextDecoration.lineThrough : null,
              ),
            ),
          ),
        ],
      ),
    ),
  );
}

class NewIdeaDialog extends StatefulWidget {
  const NewIdeaDialog({
    super.key,
    this.initialIdea,
    this.initialCategory = '灵感',
    this.readClipboard,
    this.importAttachment,
    this.plugin,
    this.editor,
    this.targetId,
    this.initialFavorite = false,
  });
  final Idea? initialIdea;
  final String? targetId;
  final bool initialFavorite;
  final WorkbenchEditorSession? editor;
  final String initialCategory;
  final Future<PastedContent> Function()? readClipboard;
  final Future<IdeaAttachment> Function(XFile)? importAttachment;
  final StudioBackend? plugin;
  @override
  State<NewIdeaDialog> createState() => _NewIdeaDialogState();
}

class _NewIdeaDialogState extends State<NewIdeaDialog> {
  final title = TextEditingController();
  final description = TextEditingController();
  String category = '灵感';
  bool invalid = false;
  final todos = TextEditingController();
  final hypothesis = TextEditingController(),
      conclusion = TextEditingController();
  final titleFocus = FocusNode(),
      descriptionFocus = FocusNode(),
      todosFocus = FocusNode(),
      hypothesisFocus = FocusNode(),
      conclusionFocus = FocusNode();
  final attachments = <IdeaAttachment>[];
  final created = <IdeaAttachment>[];
  bool importing = false, saved = false, preview = false;
  String pasteNotice = '';
  String stage = '待整理';
  late final String _targetId;
  bool saving = false, _submitFrozen = false;
  Idea? _frozenDraft;
  EditorFields? _frozenFields;
  String saveError = '';
  int _pasteSequence = 0;
  StudioBackend? get _plugin => widget.editor?.studio ?? widget.plugin;
  String _field(TextEditingController controller) => controller == title
      ? 'title'
      : controller == todos
      ? 'todos'
      : controller == hypothesis
      ? 'hypothesis'
      : controller == conclusion
      ? 'conclusion'
      : 'description';
  Future<void> _insert(
    TextEditingController target,
    String inserted,
    List<PastePart> parts, {
    int? atStart,
    int? atEnd,
  }) async {
    final before = target.text;
    final selection = target.selection;
    final start =
        atStart ?? (selection.isValid ? selection.start : before.length);
    final end = atEnd ?? (selection.isValid ? selection.end : before.length);
    final after = before.replaceRange(start, end, inserted);
    await widget.editor?.recordPaste(
      PasteInsertion(
        id: 'paste-${DateTime.now().microsecondsSinceEpoch}-${_pasteSequence++}',
        field: _field(target),
        before: before,
        startUtf16: start,
        endUtf16: end,
        parts: List.unmodifiable(
          parts.isEmpty ? [PastePart.literal(inserted)] : parts,
        ),
        after: after,
      ),
    );
    if (!mounted) return;
    if (target.text != before) throw StateError('粘贴期间输入发生变化，请重新打开编辑器。');
    target.value = TextEditingValue(
      text: after,
      selection: TextSelection.collapsed(offset: start + inserted.length),
    );
  }

  Future<void> _save() async {
    if (importing || saving) return;
    if (title.text.trim().isEmpty) {
      setState(() => invalid = true);
      return;
    }
    _frozenFields ??= EditorFields(
      title: title.text,
      description: description.text,
      hypothesis: hypothesis.text,
      conclusion: conclusion.text,
      todos: todos.text,
    );
    _frozenDraft ??= Idea(
      title.text.trim(),
      description.text.trim().isEmpty ? '从一个小小的念头开始。' : description.text.trim(),
      category,
      widget.initialIdea?.icon ?? Icons.auto_awesome_outlined,
      widget.initialIdea?.color ?? const Color(0xFF9D87D4),
      attachments: List.of(attachments),
      stage: stage,
      hypothesis: hypothesis.text.trim(),
      conclusion: conclusion.text.trim(),
      id: _targetId,
      favorite: widget.initialIdea?.favorite ?? widget.initialFavorite,
      time: widget.initialIdea?.time ?? '刚刚',
      todos: todos.text
          .split('\n')
          .map((line) => line.trim())
          .where((line) => line.isNotEmpty)
          .toSet()
          .toList(),
      completed: (widget.initialIdea?.completed ?? <String>{}).intersection(
        todos.text.split('\n').map((line) => line.trim()).toSet(),
      ),
    );
    setState(() {
      saving = true;
      _submitFrozen = true;
      saveError = '';
    });
    try {
      final result = widget.editor == null
          ? _frozenDraft!
          : await widget.editor!.save(_frozenDraft!, _frozenFields!);
      if (!mounted) return;
      saved = true;
      Navigator.pop(context, result);
    } catch (error) {
      if (mounted) {
        setState(() {
          if (error is EditorPreparationException) {
            _submitFrozen = false;
            _frozenDraft = null;
            _frozenFields = null;
            saveError = error.cause is FormatException
                ? (error.cause as FormatException).message
                : '尚未提交。草稿与附件已保留，可修改后再次保存。';
          } else {
            saveError = '这次保存尚未确认。草稿与附件已保留，请重试同一次提交；关闭后会重新读取工作台确认。';
          }
        });
      }
    } finally {
      if (mounted) setState(() => saving = false);
    }
  }

  void webPaste(ClipboardReadEvent event) {
    if (ModalRoute.of(context)?.isCurrent != true) return;
    unawaited(
      safely(() async => insertPaste(await event.getClipboardReader())),
    );
  }

  Future<void> importFiles(List<XFile> files) async {
    for (final file in files) {
      if (attachments.length >= 20) {
        throw const FormatException('每条记录最多保存 20 个附件。');
      }
      await _plugin?.validateImport(
        IdeaAttachment.kindFor(file.name).name,
        await file.length(),
        attachment: true,
      );
      final item = await (widget.importAttachment ?? IdeaAttachment.import)(
        file,
      );
      created.add(item);
      if (!mounted) {
        await TextureRepository.remove(item.source);
        return;
      }
      setState(() => attachments.add(item));
      if (title.text.trim().isEmpty) {
        final name = item.source.name.length > 60
            ? item.source.name.substring(0, 60)
            : item.source.name;
        await _insert(
          title,
          name,
          [PastePart.literal(name)],
          atStart: 0,
          atEnd: title.text.length,
        );
      }
    }
  }

  Future<void> safely(Future<void> Function() work) async {
    if (importing || saving || _submitFrozen) return;
    setState(() => importing = true);
    try {
      await work();
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              error is FormatException
                  ? error.message
                  : '无法读取内容，请使用导入文件，或检查文件与剪贴板权限。',
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => importing = false);
    }
  }

  Widget _bodyEditor(BuildContext context) {
    final p = AppearanceScope.of(context);
    final editor = TextField(
      key: const ValueKey('idea-description'),
      controller: description,
      readOnly: importing || saving || _submitFrozen,
      focusNode: descriptionFocus,
      minLines: 4,
      maxLines: 8,
      style: TextStyle(fontSize: 13, height: 1.7, color: p.ink),
      maxLength: 20000,
      decoration: const InputDecoration(
        hintText: '写下思路，或粘贴一段内容…\n\n支持 # 标题、列表、表格和代码块',
        border: InputBorder.none,
        alignLabelWithHint: true,
      ),
    );
    final rendered = ValueListenableBuilder<TextEditingValue>(
      valueListenable: description,
      builder: (context, value, _) => ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 160, maxHeight: 350),
        child: SingleChildScrollView(
          key: const ValueKey('idea-markdown-preview'),
          child: Align(
            alignment: Alignment.topLeft,
            child: value.text.trim().isEmpty
                ? Text('预览会显示在这里', style: TextStyle(color: p.muted))
                : IdeaMarkdown(data: value.text, attachments: attachments),
          ),
        ),
      ),
    );
    return DecoratedBox(
      decoration: BoxDecoration(
        color: p.ink.withValues(alpha: .025),
        border: Border.all(color: p.line),
        borderRadius: p.borderRadius(16),
      ),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          children: [
            Row(
              children: [
                Icon(Icons.notes_rounded, size: 18, color: p.accent),
                const SizedBox(width: 8),
                const Expanded(
                  child: Text(
                    '正文 · Markdown',
                    style: TextStyle(fontWeight: FontWeight.w600),
                  ),
                ),
                IconButton(
                  key: const ValueKey('idea-preview-toggle'),
                  tooltip: preview ? '收起预览' : '实时预览',
                  isSelected: preview,
                  onPressed: () => setState(() => preview = !preview),
                  icon: const Icon(Icons.visibility_outlined, size: 19),
                ),
              ],
            ),
            const Divider(height: 12),
            LayoutBuilder(
              builder: (context, constraints) {
                if (!preview) return editor;
                if (constraints.maxWidth < 590) {
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Offstage(offstage: true, child: editor),
                      rendered,
                    ],
                  );
                }
                return Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(child: editor),
                    const SizedBox(width: 24),
                    Expanded(
                      child: Padding(
                        padding: const EdgeInsets.only(top: 12),
                        child: rendered,
                      ),
                    ),
                  ],
                );
              },
            ),
          ],
        ),
      ),
    );
  }

  Future<void> paste([ClipboardReader? reader]) =>
      safely(() => insertPaste(reader));

  Future<void> insertPaste(ClipboardReader? reader) async {
    final target = titleFocus.hasFocus
        ? title
        : todosFocus.hasFocus
        ? todos
        : hypothesisFocus.hasFocus
        ? hypothesis
        : conclusionFocus.hasFocus
        ? conclusion
        : description;
    final content = reader == null && widget.readClipboard != null
        ? await widget.readClipboard!()
        : await readPaste(reader, _plugin);
    if (!mounted) return;
    final inserted = target == description && content.markdown.isNotEmpty
        ? content.markdown
        : content.text;
    if (inserted.isNotEmpty) {
      final selection = target.selection;
      final start = selection.isValid ? selection.start : target.text.length;
      final end = selection.isValid ? selection.end : target.text.length;
      final value = target.text.replaceRange(start, end, inserted);
      final limit = target == title
          ? 60
          : target == todos
          ? 1000
          : target == hypothesis
          ? 5000
          : target == conclusion
          ? 10000
          : 20000;
      if (value.characters.length > limit) {
        throw FormatException('此输入框最多 $limit 个字符，请缩短内容或将其作为文件导入。');
      }
      final parts = target == description && content.markdown.isNotEmpty
          ? content.markdownParts
          : content.textParts;
      await _insert(target, inserted, parts, atStart: start, atEnd: end);
      if (!mounted) return;
    }
    await importFiles(content.files);
    if (!mounted) return;
    // Images remain portable: Markdown refers to persisted attachments by name.
    if (target == description && inserted.isEmpty) {
      final images = attachments.where(
        (a) =>
            a.source.kind == TextureKind.image &&
            content.files.any((f) => f.name == a.source.name),
      );
      for (final item in images) {
        final uri = 'attachment:${Uri.encodeComponent(item.source.location)}';
        if (!description.text.contains(uri)) {
          final link = '\n\n![图片]($uri)';
          if ((description.text + link).characters.length <= 20000) {
            await _insert(
              description,
              link,
              [PastePart.literal(link)],
              atStart: description.text.length,
              atEnd: description.text.length,
            );
            if (!mounted) return;
          }
        }
      }
    }
    setState(() {
      preview = target == description && description.text.isNotEmpty;
      pasteNotice = content.warnings.isEmpty
          ? '已读取内容${content.files.isEmpty ? '' : '，保留 ${content.files.length} 个附件'}'
          : content.warnings.join('\n');
    });
    if (inserted.isEmpty && content.files.isEmpty && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('剪贴板中没有可读取的文本或文件。请从资源管理器复制文件，或使用导入文件。')),
      );
    }
  }

  @override
  void initState() {
    super.initState();
    _targetId =
        widget.editor?.targetId ??
        widget.initialIdea?.id ??
        widget.targetId ??
        Idea.nextId();
    if (kIsWeb) ClipboardEvents.instance?.registerPasteEventListener(webPaste);
    attachments.addAll(widget.initialIdea?.attachments ?? []);
    stage =
        widget.initialIdea?.stage ??
        (widget.initialCategory == '进行中'
            ? '推进中'
            : widget.initialCategory == '实验'
            ? '待验证'
            : '待整理');
    hypothesis.text = widget.initialIdea?.hypothesis ?? '';
    conclusion.text = widget.initialIdea?.conclusion ?? '';
    final idea = widget.initialIdea;
    title.text = idea?.title ?? '';
    description.text = idea?.description ?? '';
    category = idea?.category ?? widget.initialCategory;
    todos.text = idea?.todos.join('\n') ?? '';
  }

  @override
  void dispose() {
    unawaited(widget.editor?.close().catchError((_) {}));
    if (kIsWeb) {
      ClipboardEvents.instance?.unregisterPasteEventListener(webPaste);
    }
    for (final item in created) {
      if (!saved || !attachments.contains(item)) {
        unawaited(TextureRepository.remove(item.source).catchError((_) {}));
      }
    }
    titleFocus.dispose();
    descriptionFocus.dispose();
    todosFocus.dispose();
    hypothesisFocus.dispose();
    conclusionFocus.dispose();
    hypothesis.dispose();
    conclusion.dispose();
    title.dispose();
    description.dispose();
    todos.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !saving && !importing,
    child: Actions(
      actions: <Type, Action<Intent>>{
        PasteTextIntent: CallbackAction<PasteTextIntent>(
          onInvoke: (_) {
            unawaited(paste());
            return null;
          },
        ),
      },
      child: StudioDialog(
        canClose: !saving && !importing,
        width: 820,
        title: widget.initialIdea == null ? '接住一个新想法' : '让想法更清晰',
        subtitle: '文字、表格、图片，先放在这里。让一个念头慢慢成形。',
        content: AbsorbPointer(
          absorbing: _submitFrozen || saving || importing,
          child: SizedBox(
            width: 740,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextField(
                    key: const ValueKey('idea-title'),
                    controller: title,
                    readOnly: importing || saving || _submitFrozen,
                    focusNode: titleFocus,
                    autofocus: true,
                    maxLength: 60,
                    style: const TextStyle(
                      fontSize: 23,
                      fontWeight: FontWeight.w600,
                    ),
                    decoration: InputDecoration(
                      hintText: '给它起个名字',
                      errorText: invalid ? '先写下你的想法吧' : null,
                    ),
                  ),
                  const SizedBox(height: 12),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      FilledButton.tonalIcon(
                        key: const ValueKey('idea-paste'),
                        onPressed: importing ? null : () => paste(),
                        icon: const Icon(Icons.content_paste, size: 16),
                        label: const Text('粘贴内容'),
                      ),
                      OutlinedButton.icon(
                        key: const ValueKey('idea-add-files'),
                        onPressed: importing
                            ? null
                            : () => safely(
                                () async => importFiles(await openFiles()),
                              ),
                        icon: const Icon(Icons.attach_file, size: 16),
                        label: const Text('导入文件'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  const Text(
                    '支持 Markdown、Office 富文本与表格、截图及文件。复杂对象保留原始附件；最多 20 个附件，单个不超过 200 MB。',
                    style: TextStyle(fontSize: 11),
                  ),
                  if (pasteNotice.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 10),
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: Text(
                          pasteNotice,
                          style: TextStyle(
                            fontSize: 12,
                            color: Theme.of(context).colorScheme.primary,
                          ),
                        ),
                      ),
                    ),
                  if (importing) const LinearProgressIndicator(),
                  const SizedBox(height: 12),
                  _bodyEditor(context),
                  const SizedBox(height: 14),
                  DropdownButtonFormField<String>(
                    initialValue: category,
                    decoration: const InputDecoration(labelText: '放在哪里'),
                    items: ['灵感', '进行中', '实验']
                        .map(
                          (s) => DropdownMenuItem(
                            value: s,
                            child: Text(
                              s == '灵感'
                                  ? '灵感收件箱'
                                  : s == '进行中'
                                  ? '小项目'
                                  : '实验室',
                            ),
                          ),
                        )
                        .toList(),
                    onChanged: (v) => setState(() {
                      category = v!;
                      stage = category == '进行中'
                          ? '推进中'
                          : category == '实验'
                          ? '待验证'
                          : '待整理';
                    }),
                  ),
                  const SizedBox(height: 12),
                  ...attachments.map(
                    (item) => AttachmentTile(
                      attachment: item,
                      onRemove: importing
                          ? null
                          : () => setState(() => attachments.remove(item)),
                    ),
                  ),
                  if (category == '实验') ...[
                    const SizedBox(height: 16),
                    TextField(
                      key: const ValueKey('experiment-hypothesis'),
                      controller: hypothesis,
                      readOnly: importing || saving || _submitFrozen,
                      focusNode: hypothesisFocus,
                      minLines: 2,
                      maxLines: 4,
                      maxLength: 5000,
                      decoration: const InputDecoration(labelText: '想验证的假设'),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      key: const ValueKey('experiment-conclusion'),
                      controller: conclusion,
                      readOnly: importing || saving || _submitFrozen,
                      focusNode: conclusionFocus,
                      minLines: 2,
                      maxLines: 5,
                      maxLength: 10000,
                      decoration: const InputDecoration(labelText: '观察、过程与结论'),
                    ),
                  ],
                  const SizedBox(height: 18),
                  TextField(
                    key: const ValueKey('idea-todos'),
                    controller: todos,
                    readOnly: importing || saving || _submitFrozen,
                    focusNode: todosFocus,
                    minLines: 2,
                    maxLines: 4,
                    maxLength: 1000,
                    decoration: const InputDecoration(
                      labelText: '下一小步（每行一项，可选）',
                      alignLabelWithHint: true,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
        actions: [
          if (saveError.isNotEmpty)
            Text(
              saveError,
              key: const ValueKey('idea-save-error'),
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          TextButton(
            onPressed: importing || saving
                ? null
                : () => Navigator.pop(context),
            child: const Text('再想想'),
          ),
          FilledButton(
            key: const ValueKey('idea-save'),
            onPressed: importing || saving ? null : _save,
            child: Text(
              saving
                  ? '正在保存…'
                  : _submitFrozen
                  ? '重试保存'
                  : '保存灵感',
            ),
          ),
        ],
      ),
    ),
  );
}

class AmbientPainter extends CustomPainter {
  AmbientPainter(this.p);
  final Palette p;
  @override
  void paint(Canvas canvas, Size size) {
    if (p.backdrop == BackgroundMode.transparent) return;
    if (p.backdrop == BackgroundMode.solid) {
      canvas.drawRect(Offset.zero & size, Paint()..color = p.solidColor);
      return;
    }
    canvas.drawRect(Offset.zero & size, Paint()..color = p.background);
    if (p.backdrop == BackgroundMode.texture) {
      final grain = Paint()
        ..color = p.ink.withValues(alpha: p.dark ? .075 : .085);
      final random = math.Random(19);
      for (double y = 2; y < size.height; y += 7) {
        for (double x = 2; x < size.width; x += 7) {
          canvas.drawCircle(
            Offset(x + random.nextDouble() * 2, y + random.nextDouble() * 2),
            .45 + random.nextDouble() * .25,
            grain,
          );
        }
      }
      return;
    }
    void glow(Offset center, double radius, Color color) {
      canvas.drawCircle(
        center,
        radius,
        Paint()
          ..shader = RadialGradient(
            colors: [
              p.themeTint(color, .6).withValues(alpha: p.dark ? .25 : .42),
              color.withValues(alpha: 0),
            ],
          ).createShader(Rect.fromCircle(center: center, radius: radius)),
      );
    }

    glow(
      Offset(size.width * .53, size.height * .27),
      size.width * .44,
      const Color(0xFFC4ACEE),
    );
    glow(
      Offset(size.width * .95, size.height * .82),
      size.width * .37,
      const Color(0xFF90B4B1),
    );
    glow(
      Offset(size.width * .35, size.height * 1.03),
      size.width * .37,
      const Color(0xFFE6C3AB),
    );
    final line = Paint()
      ..color = p.accent.withValues(alpha: p.clear ? .10 : .045)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    for (var i = 0; i < 5; i++) {
      final path = Path()
        ..moveTo(size.width * .1, size.height * .88 + i * 22)
        ..cubicTo(
          size.width * .48,
          size.height * .36 + i * 25,
          size.width * .72,
          size.height * 1.2 + i * 30,
          size.width * 1.1,
          size.height * .25 + i * 40,
        );
      canvas.drawPath(path, line);
    }
  }

  @override
  bool shouldRepaint(AmbientPainter oldDelegate) =>
      oldDelegate.p.theme != p.theme ||
      oldDelegate.p.mode != p.mode ||
      oldDelegate.p.backdrop != p.backdrop ||
      oldDelegate.p.solidTint != p.solidTint ||
      oldDelegate.p.grayscale != p.grayscale ||
      oldDelegate.p.lightness != p.lightness ||
      oldDelegate.p.customColor != p.customColor ||
      oldDelegate.p.themeColor != p.themeColor;
}

class OrbPainter extends CustomPainter {
  OrbPainter(this.p);
  final Palette p;
  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width * .57, size.height * .52);
    canvas.save();
    canvas.translate(center.dx, center.dy);
    canvas.rotate(-.47);
    final rect = Rect.fromCenter(center: Offset.zero, width: 196, height: 136);
    canvas.drawOval(
      rect.shift(const Offset(0, 19)),
      Paint()
        ..color = p
            .themeTint(const Color(0xFF706283), .6)
            .withValues(alpha: .15)
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 21),
    );
    for (var i = 0; i < 48; i++) {
      final t = i / 47;
      final ring = Rect.fromCenter(
        center: Offset(0, -10 + t * 23),
        width: 202 - math.sin(t * math.pi) * 21,
        height: 133 - math.sin(t * math.pi) * 14,
      );
      canvas.drawOval(
        ring,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2.3
          ..shader = SweepGradient(
            colors: [
              p.themeTint(const Color(0xFFE5DBF0), .6).withValues(alpha: .65),
              p.themeTint(const Color(0xFF967CAF), .6).withValues(alpha: .48),
              p.themeTint(const Color(0xFFD8E4DC), .6).withValues(alpha: .83),
              Colors.white.withValues(alpha: .94),
              p.themeTint(const Color(0xFFB1A1CA), .6).withValues(alpha: .6),
              p.themeTint(const Color(0xFFE5DBF0), .6).withValues(alpha: .65),
            ],
            transform: GradientRotation(t * .7),
          ).createShader(ring),
      );
    }
    canvas.drawOval(
      Rect.fromCenter(center: const Offset(0, -8), width: 196, height: 128),
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1
        ..color = Colors.white.withValues(alpha: .85),
    );
    canvas.restore();
    final star = Paint()
      ..color = p.accent.withValues(alpha: .5)
      ..strokeWidth = 1;
    final pos = Offset(center.dx + 88, center.dy - 85);
    canvas.drawLine(pos - const Offset(0, 6), pos + const Offset(0, 6), star);
    canvas.drawLine(pos - const Offset(6, 0), pos + const Offset(6, 0), star);
  }

  @override
  bool shouldRepaint(OrbPainter oldDelegate) =>
      oldDelegate.p.theme != p.theme ||
      oldDelegate.p.mode != p.mode ||
      oldDelegate.p.themeColor != p.themeColor ||
      oldDelegate.p.grayscale != p.grayscale ||
      oldDelegate.p.lightness != p.lightness;
}
