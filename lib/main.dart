import 'theme_plugins/theme_plugin_controller.dart';
import 'theme_plugins/theme_artwork.dart';
import 'editor_draft_handoff_recovery_dialog.dart';
import 'plugins/editor_draft.dart';
import 'plugins/editor_draft_handoff_coordinator.dart';
import 'editor_draft_import_recovery_dialog.dart';
import 'plugins/editor_draft_import.dart';
import 'plugins/editor_draft_import_decision_coordinator.dart';
import 'editor_recovery_dialog.dart';
import 'plugins/editor_recovery.dart';
import 'plugins/versioned_ui_validation.dart';
import 'plugins/versioned_content.dart';
import 'plugins/versioned_idea_view.dart';
import 'plugins/versioned_editor.dart';
import 'plugins/versioned_editor_adapter.dart';
import 'versioned_task_panel.dart';
import 'fonts/font_choice.dart';
import 'save_recovery.dart';
import 'save_recovery_dialog.dart';
import 'fonts/font_repository.dart';
import 'fonts/font_settings.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'plugins/workbench_ids.dart';
import 'plugins/host_notices.dart';
import 'plugins/workbench_labels.dart';
import 'plugins/query_coordinator.dart';
import 'plugins/editor_session.dart';
import 'plugins/plugin_tools.dart';
import 'plugins/plugin_library.dart';
import 'plugins/bootstrap_stub.dart'
    if (dart.library.io) 'plugins/bootstrap_native.dart'
    if (dart.library.js_interop) 'plugins/bootstrap_web.dart'
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
import 'animated_slider_style.dart';
import 'style_depth_slider.dart';
import 'visual_style_picker.dart';
import 'neumorphic_controls.dart';
import 'experimental_controls.dart';
import 'style_input_border.dart';
import 'surface_motion.dart';
import 'component_context_menu.dart';
import 'component_material_page.dart';
import 'collapsible_panel.dart';
import 'settings_page_transition.dart';
import 'workspace_viewport.dart';
import 'settings_surface.dart';
import 'plugins/plugin_settings_page.dart';
import 'liquid_glass.dart';
import 'content/idea_markdown.dart';
import 'content/editor_attachment_rebase.dart';
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
    this.initialLocale,
    this.onFirstFrame,
    this.libraryDirectory,
    this.themePlugins,
  });
  final ThemePluginController? themePlugins;
  final Locale? initialLocale;
  final String? libraryDirectory;

  /// Signals the first laid-out workbench, after restoration and localization.
  final VoidCallback? onFirstFrame;
  final StudioStorage? storage;
  final DesktopBackground? nativeBackground;
  final String? initialWarning;
  final WorkbenchBackend? workbench;
  @override
  State<MorrowApp> createState() => _MorrowAppState();
}

class _MorrowAppState extends State<MorrowApp> {
  late final ThemePluginController themePlugins;

  void _themePluginChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    themePlugins.removeListener(_themePluginChanged);
    if (widget.themePlugins == null) themePlugins.dispose();
    super.dispose();
  }

  bool _reportedFirstFrame = false;
  String uiLocale = 'system';
  FontChoice uiFont = const FontChoice();
  String? _fontFamily;
  int _fontLoad = 0;
  AppLocalizations get l => uiLocale != 'system'
      ? L10n.forLocale(Locale(uiLocale))
      : messages.currentContext == null
      ? L10n.forLocale(L10n.fallbackLocale)
      : L10n.of(messages.currentContext!);
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
  final navigation = GlobalKey<NavigatorState>();
  bool _recoveryChecked = false;

  Future<void> reviewSaveRecovery() async {
    final recovery = storage;
    final context = navigation.currentContext;
    if (recovery is! SaveRecoveryStorage || context == null) return;
    final resolved = await showDialog<SaveRecoveryResult>(
      context: context,
      builder: (_) =>
          SaveRecoveryDialog(storage: recovery as SaveRecoveryStorage),
    );
    if (!mounted || resolved == null) return;
    if (resolved == SaveRecoveryResult.retry) {
      await saveContent(restored ?? {'ideas': [], 'completed': <String>[]});
      return;
    }
    messages.currentState?.showSnackBar(
      SnackBar(
        content: Text(l.mainSaveRecoveryDone),
        action: SnackBarAction(
          label: l.mainRetry,
          onPressed: () =>
              saveContent(restored ?? {'ideas': [], 'completed': <String>[]}),
        ),
      ),
    );
  }

  Future<void> checkSaveRecovery() async {
    if (_recoveryChecked || storage is! SaveRecoveryStorage) return;
    _recoveryChecked = true;
    try {
      final pending = await (storage as SaveRecoveryStorage)
          .inspectSaveRecovery();
      if (mounted && pending != null) {
        messages.currentState?.showSnackBar(
          SnackBar(
            content: Text(l.mainSaveRecoveryTitle),
            action: SnackBarAction(
              label: l.mainSaveRecoveryReview,
              onPressed: reviewSaveRecovery,
            ),
          ),
        );
      }
    } catch (_) {
      /* A failed background inspection never submits anything. */
    }
  }

  @override
  void initState() {
    super.initState();
    storage = widget.storage ?? MemoryStorage();
    themePlugins =
        widget.themePlugins ??
        ThemePluginController(
          backend: widget.workbench is ExternalPluginControl
              ? widget.workbench as ExternalPluginControl
              : null,
          store: storage is MemoryStorage
              ? MemoryThemePluginStore()
              : LocalThemePluginStore(),
        );
    themePlugins.addListener(_themePluginChanged);
    if (widget.themePlugins == null) unawaited(themePlugins.restore());
    warning = widget.initialWarning;
    try {
      restored = storage.read();
      final data = restored;
      try {
        uiFont = FontChoice.fromJson(data?['uiFont']);
      } catch (_) {
        uiFont = const FontChoice();
      }
      if (!uiFont.imported) _fontFamily = uiFont.resolvedFamily;
      final storedLocale = data?['uiLocale'];
      uiLocale = storedLocale is String && L10n.isSupportedCode(storedLocale)
          ? storedLocale
          : 'system';
      if (L10n.isSupportedCode(widget.initialLocale?.languageCode)) {
        uiLocale = widget.initialLocale!.languageCode;
      }
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
    if (uiFont.imported) _restoreFont();
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => applyWindowBackground(),
    );
  }

  Future<void> _restoreFont() async {
    final generation = ++_fontLoad;
    final font = uiFont;
    try {
      await FontRepository.load(
        font,
        libraryDirectory: widget.libraryDirectory,
      );
      if (mounted && generation == _fontLoad) {
        setState(() => _fontFamily = font.resolvedFamily);
      }
    } catch (_) {
      if (mounted && generation == _fontLoad) {
        setState(() => _fontFamily = null);
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted && generation == _fontLoad) {
            messages.currentState?.showSnackBar(
              SnackBar(content: Text(l.mainFontUnavailable)),
            );
          }
        });
      }
    }
  }

  Future<void> changeFont(FontChoice font) async {
    font.validate();
    final generation = ++_fontLoad;
    await FontRepository.load(font, libraryDirectory: widget.libraryDirectory);
    if (!mounted || generation != _fontLoad) return;
    setState(() {
      uiFont = font;
      _fontFamily = font.resolvedFamily;
    });
    await saveContent(restored ?? {'ideas': [], 'completed': <String>[]});
  }

  bool _initialWarningShown = false;
  void scheduleInitialWarning(BuildContext localizedContext) {
    if (_initialWarningShown || warning == null) return;
    _initialWarningShown = true;
    final translated = L10n.of(localizedContext);
    final text = switch (warning!) {
      '本地存储暂不可用，当前改动仅保留在本次会话。' => translated.mainStorageUnavailable,
      '存储内容无法读取，原数据仍保留。当前会话不会覆盖它。' => translated.mainStorageUnreadable,
      '工作台插件不可用，已有内容仍可查看和导出。' => translated.recoveryPluginUnavailable,
      final message =>
        '${translated.recoveryMaintenance}\n${translated.mainDiagnosticDetails}: $message',
    };
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      messages.currentState?.showSnackBar(
        SnackBar(content: Text(text), duration: const Duration(seconds: 8)),
      );
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
        'uiLocale': uiLocale,
        'uiFont': uiFont.toJson(),
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
    } catch (error) {
      if (mounted) {
        messages.currentState?.hideCurrentSnackBar();
        messages.currentState?.showSnackBar(
          SnackBar(
            content: Text(storageFailureNotice(l, error) ?? l.mainSaveFailed),
            action: SnackBarAction(
              label: storage is SaveRecoveryStorage
                  ? l.mainSaveRecoveryReview
                  : l.mainRetry,
              onPressed: storage is SaveRecoveryStorage
                  ? reviewSaveRecovery
                  : () => saveContent(restored ?? content),
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
                  ? l.mainFrostUnavailable
                  : l.mainTransparencyUnavailable,
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
      themePlugins.tokens(
        theme == StudioTheme.dark ||
            (theme == StudioTheme.custom && (themeLightness ?? .885) < .46),
      ),
      themePlugins.fullOverride,
    );
    final labelOverrides = WorkbenchLabelsScope.maybeOf(context);
    final app = MaterialApp(
      navigatorKey: navigation,
      scrollBehavior: const StudioScrollBehavior(),
      onGenerateTitle: (context) => L10n.of(context).mainAppTitle,
      locale: uiLocale == 'system' ? null : Locale(uiLocale),
      supportedLocales: L10n.supportedLocales,
      localizationsDelegates: const [
        L10n.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      debugShowCheckedModeBanner: false,
      scaffoldMessengerKey: messages,
      builder: (context, child) {
        scheduleInitialWarning(context);
        return FontScope(
          value: uiFont,
          onChanged: changeFont,
          child: WorkbenchLabelsScope(
            labels:
                labelOverrides ??
                WorkbenchLabels(localization: L10n.of(context)),
            child: _UiLocaleScope(
              value: uiLocale,
              onChanged: (value) {
                setState(() => uiLocale = value);
                saveContent(restored ?? {'ideas': [], 'completed': <String>[]});
              },
              child: AppearanceScope(
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
            ),
          ),
        );
      },
      theme: applyVisualStyleControls(
        ThemeData(
          useMaterial3: true,
          brightness: palette.dark ? Brightness.dark : Brightness.light,
          fontFamily: _fontFamily ?? 'Segoe UI',
          fontFamilyFallback: const ['Microsoft YaHei', 'Arial'],
          scaffoldBackgroundColor: Colors.transparent,
          snackBarTheme: const SnackBarThemeData(
            showCloseIcon: true,
            behavior: SnackBarBehavior.floating,
          ),
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
            enabledBorder: StyledInputBorder(
              borderRadius: palette.borderRadius(13),
              borderSide: BorderSide(color: palette.line),
            ),
            focusedBorder: StyledInputBorder(
              borderRadius: palette.borderRadius(13),
              borderSide: BorderSide(color: palette.accent),
            ),
            border: StyledInputBorder(borderRadius: palette.borderRadius(13)),
          ),
          popupMenuTheme: PopupMenuThemeData(
            color: palette.surface.withValues(alpha: .95),
            shape: RoundedRectangleBorder(
              borderRadius: palette.borderRadius(16),
            ),
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
        palette,
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
        onReady: (data) {
          restored = data;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) checkSaveRecovery();
          });
          if (!_reportedFirstFrame && widget.onFirstFrame != null) {
            _reportedFirstFrame = true;
            WidgetsBinding.instance.addPostFrameCallback((_) {
              if (mounted) widget.onFirstFrame?.call();
            });
          }
        },
      ),
    );
    return ThemePluginScope(controller: themePlugins, child: app);
  }
}

/// UI preference only. Updating this scope never replaces the Navigator, Studio
/// identity or an editor/controller. Saved business data keeps its original language.
class _UiLocaleScope extends InheritedWidget {
  const _UiLocaleScope({
    required this.value,
    required this.onChanged,
    required super.child,
  });
  final String value;
  final ValueChanged<String> onChanged;
  static _UiLocaleScope? of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_UiLocaleScope>();
  @override
  bool updateShouldNotify(_UiLocaleScope oldWidget) => value != oldWidget.value;
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
    this.contentRevision,
    this.contentOwner,
    this.versioned,
    this.contentDeleted = false,
    this.historicalReceipt = false,
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
  // Host-only snapshot metadata. These values are deliberately not persisted as local ideas.
  final Object? contentOwner;
  final VersionedIdeaView? versioned;
  final BigInt? contentRevision;
  final bool contentDeleted;
  final bool historicalReceipt;
  String title, description, category, stage, hypothesis, conclusion;
  final List<IdeaAttachment> attachments;
  final String time;
  final IconData icon;
  final Color color;
  final List<String> todos;
  bool favorite;
  final Set<String> completed;
  // V1 completion is a text membership relation, not a count of distinct labels.
  int get legacyCompletedCount => todos.where(completed.contains).length;
  bool get legacyAllComplete =>
      todos.isNotEmpty && todos.every(completed.contains);
  static const icons = [
    Icons.auto_awesome_outlined,
    Icons.spa_outlined,
    Icons.blur_on_rounded,
    Icons.widgets_outlined,
  ];
  Map<String, dynamic> toJson() {
    if (versioned != null) {
      throw StateError(
        'Versioned content cannot be serialized as a legacy idea',
      );
    }
    return {
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
  }

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

class _VersionedUiIntent {
  _VersionedUiIntent(this.source, this.command, this.position)
    : operation = 'workspace-${newQueryOperationId()}';
  final Idea source;
  final Object command;
  final int position;
  final String operation;
  final Stopwatch elapsed = Stopwatch()..start();
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

class _StudioState extends State<Studio> with WidgetsBindingObserver {
  EditorDraftHandoffCoordinator? _draftHandoffRecovery;
  EditorDraftHandoffCoordinator? _draftHandoffDialogOwner;
  DialogRoute<void>? _draftHandoffRoute;

  AppLocalizations get l => L10n.of(context);
  WorkbenchPage section = WorkbenchPage.overview;
  WorkbenchLabels get workbenchLabels => WorkbenchLabelsScope.of(context);
  final Map<String, int> _sampleIndexes = {};
  String categoryLabel(String value) => switch (value) {
    '灵感' => l.mainCategoryIdea,
    '进行中' => l.mainCategoryProject,
    '实验' => l.mainCategoryExperiment,
    _ => value,
  };
  String stageLabel(String value) {
    for (final stage in WorkbenchStage.values) {
      if (WorkbenchV1.stage(stage) == value) {
        return workbenchLabels.filter(StageFilter(stage));
      }
    }
    return value;
  }

  String timeLabel(String value) => switch (value) {
    '刚刚' => l.mainJustNow,
    '10 分钟前' => l.mainTenMinutesAgo,
    '1 小时前' => l.mainOneHourAgo,
    '3 小时前' => l.mainThreeHoursAgo,
    '昨天' => l.mainYesterday,
    _ => value,
  };
  String displayTitle(Idea idea) {
    final index = _sampleIndexes[idea.id];
    const originals = ['给灵感一个容器', '一个安静的数字花园', '周末，做点无用的东西', '我的桌面小助手'];
    if (index == null || idea.title != originals[index]) return idea.title;
    return [
      l.mainSampleTitle0,
      l.mainSampleTitle1,
      l.mainSampleTitle2,
      l.mainSampleTitle3,
    ][index];
  }

  String displayDescription(Idea idea) {
    final index = _sampleIndexes[idea.id];
    const originals = [
      '把突然冒出的念头放在这里。\n不急着完成，先让它发生。',
      '用小小的网页，收藏喜欢的文字、\n音乐和生活里的细枝末节。',
      '试试生成艺术，让代码长出\n意料之外的形状。',
      '一个低调常驻的伙伴，帮我记住\n那些容易忘记的小事。',
    ];
    if (index == null || idea.description != originals[index]) {
      return idea.description;
    }
    return [
      l.mainSampleBody0,
      l.mainSampleBody1,
      l.mainSampleBody2,
      l.mainSampleBody3,
    ][index];
  }

  String displayTodo(Idea idea, String todo) {
    if (!_sampleIndexes.containsKey(idea.id)) return todo;
    return switch (todo) {
      '整理第一批收藏' => l.mainSampleTodo0,
      '设计花园入口' => l.mainSampleTodo1,
      '种下一条新想法' => l.mainSampleTodo2,
      '画一个小小的原型' => l.mainSampleTodo3,
      '定义提醒交互' => l.mainSampleTodo4,
      _ => todo,
    };
  }

  WorkbenchFilter filter = GeneralFilter.all;
  String query = '';
  final search = TextEditingController();
  final quickNote = TextEditingController();
  final searchFocus = FocusNode();
  bool showAppearance = true;
  bool compactSettingsOpen = false;
  final settingsNavigator = GlobalKey<NavigatorState>();
  bool sidebarExpanded = true;
  bool showCustomTone = false;
  late final MusicController music;
  bool backgroundSound = false;
  bool backgroundAudible = false;
  int soundRevision = 0;
  bool get soundRequested =>
      !p.overridesMaterials &&
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

  Widget musicPanel() => CollapsiblePanel(
    expanded: !backgroundAudible,
    child: ComponentContextMenu(
      actions: () => [
        ComponentMenuAction(
          label: music.playing ? l.visualPauseMusic : l.visualPlayMusic,
          icon: music.playing ? Icons.pause : Icons.play_arrow,
          enabled: music.tracks.isNotEmpty && !music.blocked && !music.loading,
          onSelected: () => music.toggle(),
        ),
        ComponentMenuAction(
          label: l.visualNextTrack,
          icon: Icons.skip_next,
          enabled: music.tracks.isNotEmpty && !music.blocked && !music.loading,
          onSelected: () => music.next(),
        ),
        ComponentMenuAction(
          label: l.mainComponentSettings,
          icon: Icons.tune,
          enabled: widget.workbench == null || widget.workbench!.writable,
          onSelected: () => openComponentSettings('music', l.mainMusic),
        ),
      ],
      child: MusicPanel(key: const ValueKey('music-panel'), controller: music),
    ),
  );
  final completed = <String>{};
  WorkbenchSort sort = WorkbenchSort.recent;
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
              error is FormatException ? error.message : l.mainImportFailed,
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => importing = false);
    }
  }

  Future<void> chooseThemeColor() async {
    if (p.uiTheme != null) return;
    final previous = p.themeColor;
    final color = await showStudioDialog<Color>(
      context: context,
      completeAfterTransition: true,
      builder: (_) => ColorCompassDialog(
        initial: previous ?? p.accent,
        title: l.mainThemeCompass,
        canEdit: (palette) => palette.uiTheme == null,
        onChanged: widget.onThemeColor,
      ),
    );
    if (!mounted) return;
    final selected = p.uiTheme == null ? color : null;
    widget.onThemeColor(selected ?? previous);
    if (selected != null) widget.onAppearanceCommit();
  }

  Future<void> chooseColor() async {
    if (p.overridesMaterials) return;
    final color = await showStudioDialog<Color>(
      context: context,
      completeAfterTransition: true,
      builder: (_) => ColorCompassDialog(
        initial: p.solidColor,
        canEdit: (palette) => !palette.overridesMaterials,
      ),
    );
    if (color != null && mounted && !p.overridesMaterials) {
      widget.onColor(color);
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
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
          widget.restored!['workspaceIdeas'] is List
              ? (widget.restored!['workspaceIdeas'] as List).cast<Idea>()
              : (widget.restored!['ideas'] as List).map(
                  (raw) => Idea.fromJson(raw as Map<String, dynamic>),
                ),
        );
      completed.addAll(
        List<String>.from(widget.restored!['completed'] as List? ?? []),
      );
    }
    if (widget.workbench case WorkbenchContentRevisionSource source) {
      for (final idea in ideas) {
        if (source.knownContentRevision(idea.id) case final version?) {
          _knownContentRevisions[idea.id] = version;
        }
      }
    }
    if (widget.restored == null) {
      for (var i = 0; i < ideas.length; i++) {
        _sampleIndexes[ideas[i].id] = i;
      }
    }
    widget.onReady(snapshot());
  }

  void closeCompactSettings() {
    FocusManager.instance.primaryFocus?.unfocus();
    setState(() => compactSettingsOpen = false);
  }

  Widget compactSettings() => SettingsSurface(
    key: const ValueKey('compact-settings-page'),
    title: l.mainSettings,
    onBack: closeCompactSettings,
    backKey: const ValueKey('compact-settings-back'),
    backTooltip: l.mainBackToWorkbench,
    child: SingleChildScrollView(
      key: const PageStorageKey('compact-settings-scroll'),
      padding: const EdgeInsets.only(bottom: 20),
      child: SettingsSections(
        children: [
          appearance(),
          Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [scratchpad(), const SizedBox(height: 20), musicPanel()],
          ),
        ],
      ),
    ),
  );

  Map<String, dynamic> snapshot() => {
    // Native content is only a transient presentation, never preference JSON.
    if (widget.workbench is WorkbenchMixedContent)
      'workspaceIdeas': List<Idea>.unmodifiable(ideas),
    'ideas': widget.workbench is WorkbenchMixedContent
        ? <Map<String, dynamic>>[]
        : ideas.map((idea) => idea.toJson()).toList(),
    'completed': completed.toList(),
    'music': music.toJson(),
    'sidebarExpanded': sidebarExpanded,
    'appearanceExpanded': showAppearance,
  };
  void persist() {
    _localViews.clear();
    widget.onSave(snapshot());
  }

  @override
  void didHaveMemoryPressure() {
    _localViews.clear();
    _queries.releaseCompletedViews();
    clearPluginCatalogSnapshots();
  }

  void refreshPage(VoidCallback update) => setState(update);

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _queries.dispose();
    _dismissDraftHandoffRoute();
    _draftHandoffRecovery?.dispose();
    music.dispose();
    search.dispose();
    quickNote.dispose();
    searchFocus.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(Studio oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(widget.workbench, oldWidget.workbench)) {
      _dismissDraftHandoffRoute();
      _draftHandoffRecovery?.dispose();
      _draftHandoffRecovery = null;
      _draftHandoffDialogOwner = null;
      _localViews.clear();
      _frameIdeas = null;
      _knownContentRevisions.clear();
      _pluginBusyOwner = null;
      _pendingVersioned.clear();
      _contentGeneration++;
      // A reused Studio State must never project workspace A cards through B's
      // query IDs, or persist those cards as B's preferences while B loads.
      ideas.clear();
      if (widget.workbench case WorkbenchEditorSupport _) {
        final backend = widget.workbench;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted && identical(widget.workbench, backend)) {
            unawaited(_refreshAfterEditorClose(backend));
          }
        });
      } else if (!identical(widget.restored, oldWidget.restored)) {
        if (widget.restored?['ideas'] case final List restoredIdeas) {
          ideas.addAll(
            restoredIdeas.map(
              (raw) => Idea.fromJson(raw as Map<String, dynamic>),
            ),
          );
        }
      }
    }
    if (!soundRequested) {
      backgroundAudible = false;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) music.setBlocked(false);
      });
    }
  }

  WorkbenchBackend? _pluginBusyOwner;
  final _pendingVersioned = <String, _VersionedUiIntent>{};
  final _knownContentRevisions = <String, BigInt>{};
  int _contentGeneration = 0;

  bool _acceptContentResult(
    Idea result, {
    required bool remove,
    int? position,
  }) {
    final version = result.contentRevision;
    final known = _knownContentRevisions[result.id];
    if (version != null && known != null && version < known) return false;
    if (version != null) {
      if (widget.workbench case WorkbenchContentRevisionSource source) {
        final latest = source.knownContentRevision(result.id);
        if (latest != null && version < latest) return false;
      }
    }
    if (version != null) _knownContentRevisions[result.id] = version;
    setState(() {
      final index = ideas.indexWhere((item) => item.id == result.id);
      if (remove) {
        if (index >= 0) ideas.removeAt(index);
      } else if (index >= 0) {
        ideas[index] = result;
      } else {
        ideas.insert((position ?? 0).clamp(0, ideas.length), result);
      }
      _contentGeneration++;
    });
    persist();
    return true;
  }

  late final _queries = QueryCoordinator(
    onChanged: () {
      if (mounted) setState(() {});
    },
  );
  bool _searchComposing = false;
  Future<Idea> versionedChange(
    Idea idea,
    Object command, {
    int? position,
  }) async {
    final backend = widget.workbench;
    if (idea.contentOwner != null && !identical(idea.contentOwner, backend)) {
      throw const VersionedMutationNotSubmitted();
    }
    if (backend is! WorkbenchMixedContent ||
        !backend!.writable ||
        idea.versioned == null ||
        _pluginBusyOwner != null) {
      throw StateError('Versioned mutation is unavailable');
    }
    final mixed = backend as WorkbenchMixedContent;
    final pending = _pendingVersioned[idea.id];
    if (pending == null) {
      // Only this pre-send branch can safely release an invalid draft.
      if (backend case WorkbenchContentRevisionSource revisions) {
        final known = revisions.knownContentRevision(idea.id);
        if (known != null && known > idea.versioned!.revision) {
          await _refreshAfterEditorClose(backend);
          throw const VersionedMutationNotSubmitted();
        }
      }
      if (command is TaskEditCommand) {
        validateVersionedTaskDraft(idea.versioned!.source, command);
      }
    }
    if (pending != null && !identical(pending.command, command)) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(l.mainSaveUnknown),
          action: SnackBarAction(
            label: l.mainRetry,
            onPressed: () {
              if (mounted && identical(widget.workbench, backend)) {
                unawaited(
                  versionedChange(
                    pending.source,
                    pending.command,
                    position: position,
                  ).catchError((Object _) => pending.source),
                );
              }
            },
          ),
        ),
      );
      throw StateError('Resolve the original versioned operation first');
    }
    final intent =
        pending ??
        _VersionedUiIntent(
          idea,
          command,
          position ?? ideas.indexWhere((item) => item.id == idea.id),
        );
    _pendingVersioned[idea.id] = intent;
    _pluginBusyOwner = backend;
    try {
      final current = switch (intent.command) {
        final TaskEditCommand task => await mixed.applyWorkspaceTask(
          intent.operation,
          intent.source,
          task,
        ),
        final CardEditCommand card => await mixed.applyWorkspaceCard(
          intent.operation,
          intent.source,
          card,
        ),
        _ => throw StateError('Unsupported versioned UI operation'),
      };
      if (!mounted || !identical(widget.workbench, backend)) return current;
      _pendingVersioned.remove(idea.id);
      if (!_acceptContentResult(
        current,
        remove: current.contentDeleted,
        position: position,
      )) {
        unawaited(_refreshAfterEditorClose(backend));
        throw const WorkbenchCommittedRefreshFailure('Newer view required');
      }
      if (intent.command case CardEditCommand(kind: CardEditKind.delete)) {
        if (current.contentDeleted) {
          _showVersionedDeleteUndo(
            current,
            backend,
            intent.position,
            intent.elapsed,
          );
        }
      }
      return current;
    } catch (error) {
      final noCommit =
          error is VersionedMutationNoCommit &&
          error.id == intent.source.id &&
          error.operation == intent.operation &&
          error.sourceRevision == intent.source.versioned?.revision;
      if (error is VersionedMutationNoCommit && !noCommit) {
        throw StateError('Uncorrelated versioned no-commit result');
      }
      final locallyUnsent =
          error is VersionedMutationNotSubmitted && pending == null;
      if (mounted &&
          identical(widget.workbench, backend) &&
          (noCommit || locallyUnsent)) {
        _pendingVersioned.remove(idea.id);
        if (noCommit) await _refreshAfterEditorClose(backend);
      }
      if (mounted &&
          identical(widget.workbench, backend) &&
          (!(noCommit || locallyUnsent) || intent.command is CardEditCommand)) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              noCommit || locallyUnsent
                  ? l.mainSaveNotSubmitted
                  : error is WorkbenchCommittedRefreshFailure
                  ? l.mainContentCommittedRefreshFailed
                  : l.mainSaveUnknown,
            ),
            action: noCommit || locallyUnsent
                ? null
                : SnackBarAction(
                    label: l.mainRetry,
                    onPressed: () {
                      if (!mounted || !identical(widget.workbench, backend)) {
                        return;
                      }
                      if (_pendingVersioned[idea.id] == intent) {
                        unawaited(
                          versionedChange(
                            intent.source,
                            intent.command,
                            position: position,
                          ).catchError((Object _) => intent.source),
                        );
                      } else {
                        unawaited(_refreshAfterEditorClose(backend));
                      }
                    },
                  ),
          ),
        );
      }
      rethrow;
    } finally {
      if (identical(_pluginBusyOwner, backend)) _pluginBusyOwner = null;
    }
  }

  void _showVersionedDeleteUndo(
    Idea deleted,
    WorkbenchBackend backend,
    int position,
    Stopwatch elapsed,
  ) {
    // Host uses its own monotonic clock. Starting before dispatch gives a
    // conservative remaining window without comparing unrelated clock epochs.
    final remaining = 8000 - elapsed.elapsedMilliseconds;
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(l.mainDeleted(displayTitle(deleted))),
        duration: Duration(milliseconds: remaining > 0 ? remaining : 3000),
        action: remaining <= 0
            ? null
            : SnackBarAction(
                label: l.mainUndo,
                onPressed: () {
                  if (!mounted ||
                      !identical(widget.workbench, backend) ||
                      elapsed.elapsedMilliseconds >= 8000 ||
                      ideas.any((item) => item.id == deleted.id)) {
                    return;
                  }
                  unawaited(
                    pluginChange(
                      PluginAction.restore,
                      deleted,
                      position: position,
                    ),
                  );
                },
              ),
      ),
    );
  }

  Future<Idea?> pluginChange(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
    int? position,
  }) async {
    final backend = widget.workbench;
    if (backend == null ||
        _pluginBusyOwner != null ||
        (idea.contentOwner != null && !identical(idea.contentOwner, backend))) {
      return null;
    }
    if (idea.versioned != null) {
      final Object command = switch (action) {
        PluginAction.favorite => CardEditCommand.setFavorite(flag),
        PluginAction.stage => TaskEditCommand.setStage(text),
        PluginAction.toProject => const CardEditCommand.setCategory(
          '进行中',
          '计划中',
        ),
        PluginAction.delete => const CardEditCommand.delete(),
        PluginAction.restore => const CardEditCommand.restore(),
        _ => throw StateError('Use the typed editor or TaskId action'),
      };
      try {
        return await versionedChange(idea, command, position: position);
      } catch (_) {
        return null;
      }
    }
    _pluginBusyOwner = backend;
    try {
      final result = await backend.apply(action, idea, text: text, flag: flag);
      if (!mounted || !identical(widget.workbench, backend)) return null;
      final remove = result.contentRevision == null
          ? action == PluginAction.delete
          : result.contentDeleted;
      if (_acceptContentResult(result, remove: remove, position: position)) {
        return result;
      }
      if (backend is WorkbenchEditorSupport) {
        unawaited(_refreshAfterEditorClose(backend));
      }
      return null;
    } catch (error) {
      if (mounted && identical(widget.workbench, backend)) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              error is WorkbenchCommittedRefreshFailure
                  ? l.mainContentCommittedRefreshFailed
                  : storageFailureNotice(l, error) ?? l.mainChangeFailed,
            ),
            duration: const Duration(seconds: 30),
            action:
                error is WorkbenchCommittedRefreshFailure ||
                    backend is WorkbenchMutationFailureNeedsRefresh
                ? SnackBarAction(
                    label: l.mainRetry,
                    onPressed: () => _refreshAfterEditorClose(backend),
                  )
                : SnackBarAction(
                    label: l.mainRetry,
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
      if (identical(_pluginBusyOwner, backend)) _pluginBusyOwner = null;
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

  final _localViews =
      <
        ({
          WorkbenchPage page,
          WorkbenchFilter filter,
          String text,
          WorkbenchSort sort,
        }),
        List<Idea>
      >{};
  List<Idea>? _frameIdeas;
  List<Idea> get visibleIdeas => _frameIdeas ??= _calculateVisibleIdeas();

  List<Idea> _calculateVisibleIdeas() {
    pluginProjection();
    if (widget.workbench?.writable ?? false) {
      final byId = {for (final idea in ideas) idea.id: idea};
      return [
        for (final id in _queries.ids)
          if (byId[id] != null) byId[id]!,
      ];
    }

    final conditions = (page: section, filter: filter, text: query, sort: sort);
    final cached = _localViews.remove(conditions);
    if (cached != null) {
      _localViews[conditions] = cached;
      return cached;
    }
    final needle = query.toLowerCase();
    final result = ideas.where((idea) {
      final sectionMatches = switch (section) {
        WorkbenchPage.inbox => idea.category == '灵感',
        WorkbenchPage.projects => idea.category == '进行中',
        WorkbenchPage.laboratory => idea.category == '实验',
        WorkbenchPage.favorites => idea.favorite,
        _ => true,
      };
      return sectionMatches &&
          matchesPageFilter(idea) &&
          (needle.isEmpty ||
              '${idea.title} ${idea.description} ${idea.hypothesis} ${idea.conclusion} ${idea.attachments.map((a) => a.source.name).join(' ')}'
                  .toLowerCase()
                  .contains(needle));
    }).toList();
    if (sort == WorkbenchSort.title) {
      result.sort((a, b) => a.title.compareTo(b.title));
    }
    if (sort == WorkbenchSort.favoritesFirst) {
      result.sort((a, b) => (b.favorite ? 1 : 0).compareTo(a.favorite ? 1 : 0));
    }
    _localViews[conditions] = List.unmodifiable(result);
    while (_localViews.length > 8 ||
        _localViews.values.fold<int>(0, (n, v) => n + v.length) > 20000) {
      _localViews.remove(_localViews.keys.first);
    }
    return result;
  }

  @override
  Widget build(BuildContext context) {
    _frameIdeas = null;
    return CallbackShortcuts(
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
                    child:
                        p.uiTheme != null &&
                            p.backdrop == BackgroundMode.ambient
                        ? ThemeCanvas(
                            key: const ValueKey('theme-plugin-canvas'),
                            tokens: p.uiTheme!,
                          )
                        : CustomPaint(painter: AmbientPainter(p)),
                  ),
                ),
              ),
            ),
            if (!p.overridesMaterials) Positioned.fill(child: mediaCanvas()),
            if (!p.overridesMaterials &&
                p.backdrop == BackgroundMode.transparent)
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
            if (p.liquidCanvas && !p.overridesMaterials)
              Positioned.fill(
                child: IgnorePointer(
                  child: LiquidGlassSurface(
                    key: const ValueKey('liquid-canvas'),
                    canvas: true,
                    transparentCanvas: p.backdrop == BackgroundMode.transparent,
                    tint: p.surface,
                    dark: p.dark,
                    borderRadius: BorderRadius.circular(p.windowRadius),
                    child: const SizedBox.expand(),
                  ),
                ),
              ),
            if (p.overridesMaterials && p.backdrop != BackgroundMode.ambient)
              Positioned.fill(
                child: ThemeCanvas(
                  key: const ValueKey('theme-plugin-canvas'),
                  tokens: p.uiTheme!,
                ),
              ),
            SafeArea(
              minimum: EdgeInsets.only(top: widget.desktopCaption ? 32 : 0),
              child: CanvasNavigation(
                navigatorKey: settingsNavigator,
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final desktop = constraints.maxWidth >= 1050;
                    final sidebar = constraints.maxWidth >= 760;
                    return PopScope(
                      canPop: !compactSettingsOpen,
                      onPopInvokedWithResult: (didPop, result) {
                        if (!didPop && compactSettingsOpen) {
                          closeCompactSettings();
                        }
                      },
                      child: SettingsPageTransition(
                        showSettings: compactSettingsOpen,
                        duration: motionDuration(context, 320),
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
                                child: Stack(
                                  fit: StackFit.expand,
                                  children: [
                                    Column(
                                      children: [
                                        header(sidebar, desktop),
                                        if (widget.workbench != null &&
                                            !widget.workbench!.writable) ...[
                                          const SizedBox(height: 12),
                                          readOnlyNotice(),
                                        ],
                                        const SizedBox(height: 22),
                                        Expanded(
                                          child: Row(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.start,
                                            children: [
                                              Expanded(
                                                child: workspaceViewport(),
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
                                                    padding:
                                                        const EdgeInsets.only(
                                                          left: 24,
                                                        ),
                                                    child:
                                                        SingleChildScrollView(
                                                          child: Column(
                                                            children: [
                                                              appearance(),
                                                              const SizedBox(
                                                                height: 20,
                                                              ),
                                                              scratchpad(),
                                                              const SizedBox(
                                                                height: 20,
                                                              ),
                                                              musicPanel(),
                                                              const SizedBox(
                                                                height: 20,
                                                              ),
                                                              smallQuote(),
                                                            ],
                                                          ),
                                                        ),
                                                  ),
                                                ),
                                            ],
                                          ),
                                        ),
                                      ],
                                    ),
                                    AnimatedPositioned(
                                      duration: motionDuration(context, 280),
                                      left: 0,
                                      right: desktop && showAppearance
                                          ? 276
                                          : 0,
                                      bottom: 0,
                                      child: FooterOverlay(music: music),
                                    ),
                                  ],
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget navigation() => Glass(
    componentId: 'navigation',
    p: p,
    radius: 26,
    child: LayoutBuilder(
      builder: (context, constraints) => SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: IntrinsicHeight(
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
                      l.mainSidebarMotto,
                      style: TextStyle(
                        fontSize: 10,
                        color: p.muted,
                        letterSpacing: 1,
                      ),
                    ),
                  ),
                  const SizedBox(height: 42),
                  Padding(
                    padding: const EdgeInsets.only(left: 13, bottom: 14),
                    child: Text(
                      l.mainMySpace,
                      style: TextStyle(
                        fontSize: 10,
                        color: p.muted,
                        letterSpacing: 1.5,
                      ),
                    ),
                  ),
                  navItem(WorkbenchPage.overview, Icons.grid_view_rounded),
                  navItem(
                    WorkbenchPage.inbox,
                    Icons.inbox_outlined,
                    count: ideas.where((e) => e.category == '灵感').length,
                  ),
                  navItem(WorkbenchPage.projects, Icons.folder_open_rounded),
                  navItem(WorkbenchPage.laboratory, Icons.science_outlined),
                  const SizedBox(height: 14),
                  Divider(color: p.line, indent: 12, endIndent: 12),
                  const SizedBox(height: 14),
                  navItem(
                    WorkbenchPage.favorites,
                    Icons.bookmark_border_rounded,
                  ),
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
                        Icon(
                          Icons.wb_twilight_rounded,
                          color: p.accent,
                          size: 23,
                        ),
                        const SizedBox(height: 9),
                        RotatingTip(
                          key: const ValueKey('corner-tips'),
                          lines: localizedCornerTips(context),
                          style: TextStyle(
                            fontSize: 10,
                            color: p.muted,
                            height: 1.8,
                          ),
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
                            Text(
                              l.mainPersonalWorkspace,
                              style: TextStyle(fontSize: 11, color: p.ink),
                            ),
                            Text(
                              l.mainSidebarMotto,
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
          ),
        ),
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

  Widget navItem(WorkbenchPage page, IconData icon, {int? count}) {
    final title = workbenchLabels.page(page);
    final selected = section == page;
    return Padding(
      padding: const EdgeInsets.only(bottom: 5),
      child: NeumorphicSurface(
        palette: p,
        depth: selected ? -1 : 0,
        borderRadius: p.borderRadius(12),
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
              key: ValueKey('nav-${page.id}'),
              onTap: () => selectSection(page),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 13,
                ),
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
      ),
    );
  }

  void selectSection(WorkbenchPage value) => setState(() {
    FocusManager.instance.primaryFocus?.unfocus();
    section = value;
    filter = GeneralFilter.all;
  });

  Widget header(bool sidebar, bool desktop) => Row(
    children: [
      if (!sidebar)
        PopupMenuButton<WorkbenchPage>(
          tooltip: l.mainNavigation,
          onSelected: selectSection,
          icon: Icon(Icons.menu_rounded, color: p.ink),
          itemBuilder: (_) => WorkbenchPage.values
              .map(
                (page) => PopupMenuItem(
                  value: page,
                  child: Text(workbenchLabels.page(page)),
                ),
              )
              .toList(),
        )
      else ...[
        IconButton(
          key: const ValueKey('sidebar-toggle'),
          tooltip: sidebarExpanded
              ? l.mainCollapseSidebar
              : l.mainExpandSidebar,
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
        Expanded(
          child: Row(
            children: [
              Icon(Icons.space_dashboard_outlined, size: 16, color: p.muted),
              const SizedBox(width: 9),
              Flexible(
                child: Text(
                  l.mainWorkbench,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: p.muted, fontSize: 11),
                ),
              ),
              const SizedBox(width: 10),
              Text('/', style: TextStyle(color: p.muted)),
              const SizedBox(width: 10),
              Flexible(
                child: Text(
                  workbenchLabels.page(section),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: p.ink, fontSize: 11),
                ),
              ),
              const SizedBox(width: 10),
            ],
          ),
        ),
      ],
      if (sidebar)
        SizedBox(width: 246, child: searchField())
      else
        Expanded(child: searchField()),
      const SizedBox(width: 8),
      if (widget.workbench is WorkbenchEditorDraftHandoffProposalSupport ||
          (widget.workbench is WorkbenchEditorRecovery &&
              widget.workbench is WorkbenchEditorDraftImportSupport))
        PopupMenuButton<String>(
          key: const ValueKey('editor-recovery-open'),
          tooltip: l.mainEditorRecoveryTitle,
          icon: Icon(Icons.history_rounded, size: 19, color: p.muted),
          onSelected: (value) {
            if (value == 'handoffs') {
              unawaited(reviewDraftHandoffRecovery());
            } else if (value == 'imports') {
              unawaited(reviewDraftImportRecovery());
            } else {
              unawaited(reviewEditorRecovery());
            }
          },
          itemBuilder: (_) => [
            if (widget.workbench is WorkbenchEditorRecovery)
              PopupMenuItem(
                key: const ValueKey('editor-recovery-menu-edits'),
                value: 'edits',
                child: Text(l.mainEditorRecoveryTitle),
              ),
            if (widget.workbench is WorkbenchEditorDraftImportSupport)
              PopupMenuItem(
                key: const ValueKey('editor-recovery-menu-imports'),
                value: 'imports',
                child: Text(l.mainDraftImportRecoveryTitle),
              ),
            if (widget.workbench is WorkbenchEditorDraftHandoffProposalSupport)
              PopupMenuItem(
                key: const ValueKey('editor-recovery-menu-handoffs'),
                value: 'handoffs',
                child: Text(l.mainDraftHandoffRecoveryTitle),
              ),
          ],
        )
      else if (widget.workbench is WorkbenchEditorRecovery)
        IconButton(
          key: const ValueKey('editor-recovery-open'),
          tooltip: l.mainEditorRecoveryTitle,
          onPressed: reviewEditorRecovery,
          icon: Icon(Icons.history_rounded, size: 19, color: p.muted),
        )
      else if (widget.workbench is WorkbenchEditorDraftImportSupport)
        IconButton(
          key: const ValueKey('draft-import-recovery-open'),
          tooltip: l.mainDraftImportRecoveryTitle,
          onPressed: reviewDraftImportRecovery,
          icon: Icon(Icons.history_rounded, size: 19, color: p.muted),
        ),
      IconButton(
        key: const ValueKey('appearance-toggle'),
        tooltip: desktop && showAppearance
            ? l.mainHideAppearance
            : l.mainShowAppearance,
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
      recessed: true,
      p: p,
      radius: 12,
      child: TextField(
        controller: search,
        focusNode: searchFocus,
        style: TextStyle(fontSize: 11, color: p.ink),
        decoration: InputDecoration(
          prefixIcon: Icon(Icons.search, size: 17, color: p.muted),
          hintText: l.mainSearchHint,
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
                  tooltip: l.mainClearSearch,
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
              section == WorkbenchPage.overview
                  ? l.mainGreeting
                  : workbenchLabels.page(section),
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
              l.mainGreetingDetail,
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
        label: Text(l.mainNewIdea, style: TextStyle(fontSize: 11)),
      ),
    ],
  );

  Widget hero() => LayoutBuilder(
    builder: (context, constraints) => Glass(
      componentId: 'hero',
      p: p,
      radius: 23,
      child: ConstrainedBox(
        constraints: BoxConstraints(minHeight: p.uiTheme == null ? 214 : 254),
        child: Stack(
          children: [
            if (p.uiTheme != null &&
                ThemePluginScope.of(context)?.plugin != null)
              Positioned.fill(
                child: ClipRRect(
                  borderRadius: p.borderRadius(23),
                  child: Align(
                    alignment: Alignment.centerRight,
                    child: SizedBox(
                      width: constraints.maxWidth < 520
                          ? constraints.maxWidth
                          : math.min(constraints.maxWidth * .57, 381),
                      height: double.infinity,
                      child: Opacity(
                        opacity: constraints.maxWidth < 520 ? .18 : 1,
                        child: ThemeArtwork(
                          plugin: ThemePluginScope.of(context)!.plugin!,
                        ),
                      ),
                    ),
                  ),
                ),
              )
            else
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
            Padding(
              padding: const EdgeInsets.all(25),
              child: Column(
                mainAxisSize: MainAxisSize.min,
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
                        ThemePluginScope.of(context)?.plugin?.caption(
                              Localizations.localeOf(context),
                            ) ??
                            l.mainHeroCaption,
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
                    l.mainHeroTitle,
                    style: TextStyle(
                      fontSize: 21,
                      fontWeight: FontWeight.w500,
                      color: p.ink,
                    ),
                  ),
                  const SizedBox(height: 10),
                  Text(
                    l.mainHeroBody,
                    style: TextStyle(
                      fontSize: 11,
                      height: 1.85,
                      color: p.muted,
                    ),
                  ),
                  const SizedBox(height: 20),
                  InkWell(
                    onTap: createIdea,
                    borderRadius: p.borderRadius(8),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 5),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            l.mainCaptureNow,
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
            section == WorkbenchPage.overview
                ? l.mainRecentThoughts
                : workbenchLabels.page(section),
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
      Wrap(
        spacing: 4,
        runSpacing: 4,
        children: pageFilters
            .map(
              (label) => Padding(
                padding: const EdgeInsets.only(left: 4),
                child: InkWell(
                  borderRadius: p.borderRadius(8),
                  key: ValueKey('filter-${label.id}'),
                  onTap: () => setState(() => filter = label),
                  child: NeumorphicSurface(
                    palette: p,
                    depth: filter == label ? -0.8 : 0,
                    borderRadius: p.borderRadius(8),
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
                        workbenchLabels.filter(label),
                        style: TextStyle(
                          fontSize: 10,
                          color: filter == label ? p.accent : p.muted,
                        ),
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

  Widget workspaceViewport() {
    final items = visibleIdeas;
    return WorkspaceViewport(
      page: section.id,
      session: widget.workbench ?? this,
      queryPending:
          (widget.workbench?.writable ?? false) &&
          _queries.phase == QueryPhase.loading,
      cardRegionKey: ValueKey(switch (section) {
        WorkbenchPage.inbox => 'inbox-list',
        WorkbenchPage.projects => 'project-board',
        WorkbenchPage.laboratory => 'experiment-journal',
        WorkbenchPage.favorites => 'favorites-library',
        _ => 'overview-cards',
      }),
      ids: [for (final idea in items) idea.id],
      twoColumnWidth: switch (section) {
        WorkbenchPage.overview => 460,
        WorkbenchPage.projects => 660,
        _ => double.infinity,
      },
      status: cardsStatus(items),
      header: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          greeting(),
          const SizedBox(height: 24),
          if (section == WorkbenchPage.overview) hero() else pageIntro(),
          const SizedBox(height: 28),
          collectionHeader(),
          const SizedBox(height: 6),
          Align(
            alignment: Alignment.centerRight,
            child: PopupMenuButton<WorkbenchSort>(
              key: const ValueKey('workbench-sort'),
              tooltip: l.mainArrangeIdeas,
              initialValue: sort,
              onSelected: (value) => setState(() => sort = value),
              itemBuilder: (_) =>
                  [
                        WorkbenchSort.recent,
                        WorkbenchSort.favoritesFirst,
                        WorkbenchSort.title,
                      ]
                      .map(
                        (label) => PopupMenuItem(
                          value: label,
                          child: Text(workbenchLabels.sort(label)),
                        ),
                      )
                      .toList(),
              child: Padding(
                padding: const EdgeInsets.all(6),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.sort_rounded, size: 13, color: p.muted),
                    const SizedBox(width: 5),
                    Text(
                      workbenchLabels.sort(sort),
                      style: TextStyle(fontSize: 10, color: p.muted),
                    ),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
      itemBuilder: (_, index) => section == WorkbenchPage.overview
          ? ideaCard(items[index])
          : specializedCard(items[index], index),
      footer: Padding(
        padding: const EdgeInsets.only(top: 22, bottom: 94),
        child: quickCapture(),
      ),
    );
  }

  Widget? cardsStatus(List<Idea> items) {
    if ((widget.workbench?.writable ?? false) &&
        _queries.phase != QueryPhase.ready) {
      final failed = _queries.phase == QueryPhase.failed;
      final capacity = failed && (_queries.failure?.capacity ?? false);
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
                      ? capacity
                            ? l.mainQueryCapacity
                            : (_queries.failure?.terminal ?? false)
                            ? l.mainQueryTerminated
                            : l.mainQueryUnknown
                      : l.mainQueryLoading,
                  key: ValueKey(failed ? 'query-error' : 'query-loading'),
                  style: TextStyle(color: p.muted),
                ),
                if (capacity)
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 16,
                      vertical: 8,
                    ),
                    child: Text(
                      l.mainQueryCapacityDetail,
                      textAlign: TextAlign.center,
                      style: TextStyle(color: p.muted, fontSize: 12),
                    ),
                  ),
                if (failed)
                  TextButton(
                    key: const ValueKey('query-retry'),
                    onPressed: _queries.retry,
                    child: Text(
                      capacity
                          ? l.mainCheckAgain
                          : (_queries.failure?.terminal ?? false)
                          ? l.mainQueryAgain
                          : l.mainQueryRetry,
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
                Text(l.mainNoMatches, style: TextStyle(color: p.muted)),
                TextButton(
                  onPressed: () => setState(() {
                    search.clear();
                    query = '';
                    filter = GeneralFilter.all;
                    section = WorkbenchPage.overview;
                  }),
                  child: Text(l.mainViewAll),
                ),
              ],
            ),
          ),
        ),
      );
    }
    return null;
  }

  Widget ideaCard(Idea idea) => cardInteractions(
    idea,
    Glass(
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
                          ? l.mainUnfavoriteTooltip(displayTitle(idea))
                          : l.mainFavoriteTooltip(displayTitle(idea)),
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
                  displayTitle(idea),
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
                  displayDescription(idea),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 11, color: p.muted, height: 1.8),
                ),
                if (idea.attachments.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 10),
                    child: Text(
                      l.mainCardAttachments(
                        idea.attachments.length,
                        idea.attachments.first.source.name,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 10, color: p.accent),
                    ),
                  ),
                const SizedBox(height: 19),
                Wrap(
                  alignment: WrapAlignment.spaceBetween,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: 12,
                  runSpacing: 12,
                  children: [
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 7,
                        vertical: 4,
                      ),
                      decoration: BoxDecoration(
                        color: p
                            .componentColor(idea.color)
                            .withValues(alpha: .1),
                        borderRadius: p.borderRadius(5),
                      ),
                      child: Text(
                        categoryLabel(idea.category),
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
                    Text(
                      timeLabel(idea.time),
                      style: TextStyle(fontSize: 9, color: p.muted),
                    ),
                  ],
                ),
              ],
            ),
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
                hintText: l.mainQuickHint,
                hintStyle: TextStyle(fontSize: 11, color: p.muted),
                border: InputBorder.none,
              ),
            ),
          ),
          IconButton(
            tooltip: l.mainCaptureIdea,
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
          section == WorkbenchPage.projects
              ? '进行中'
              : section == WorkbenchPage.laboratory
              ? '实验'
              : '灵感',
          Icons.auto_awesome_outlined,
          const Color(0xFF9D87D4),
          favorite: section == WorkbenchPage.favorites,
        ),
      );
      if (result != null && mounted) {
        setState(() {
          quickNote.clear();
          filter = GeneralFilter.all;
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
          section == WorkbenchPage.projects
              ? '进行中'
              : section == WorkbenchPage.laboratory
              ? '实验'
              : '灵感',
          Icons.auto_awesome_outlined,
          const Color(0xFF9D87D4),
          favorite: section == WorkbenchPage.favorites,
        ),
      );
      quickNote.clear();
      filter = GeneralFilter.all;
      query = '';
      search.clear();
    });
    persist();
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(l.mainIdeaSaved), duration: Duration(seconds: 2)),
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
      AnimatedSliderStyle(
        palette: p,
        flatThumbRadius: 6,
        flatTrackHeight: 3,
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
    if (p.overridesMaterials || (!canvas && p.uiTheme != null)) return;
    final before = p.surfaces;
    final chosen = await showStudioDialog<Color>(
      context: context,
      completeAfterTransition: true,
      builder: (_) => ColorCompassDialog(
        title: canvas ? l.mainCanvasCompass : l.mainComponentCompass,
        canEdit: (palette) =>
            !palette.overridesMaterials && (canvas || palette.uiTheme == null),
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
    final selected = !p.overridesMaterials && (canvas || p.uiTheme == null)
        ? chosen
        : null;
    updateSurfaces(
      selected == null
          ? before
          : canvas
          ? before.copyWith(canvasColor: selected)
          : before.copyWith(componentColor: selected),
    );
    if (selected != null) widget.onAppearanceCommit();
  }

  Widget surfaceColorButton(bool canvas) => OutlinedButton.icon(
    key: ValueKey(canvas ? 'canvas-color' : 'component-color'),
    onPressed: () => chooseSurfaceColor(canvas),
    icon: Icon(Icons.palette_outlined, size: 16, color: p.accent),
    label: Text(l.mainCustomCompass, style: TextStyle(fontSize: 11)),
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
        l.mainFrostEffect,
        p.surfaces.canvasBlur,
        40,
        (value) => updateSurfaces(p.surfaces.copyWith(canvasBlur: value)),
        enabled: isWindowsDesktop,
      ),
      materialSlider(
        'canvas-opacity',
        l.mainTintOpacity,
        p.surfaces.canvasOpacity,
        1,
        (value) => updateSurfaces(p.surfaces.copyWith(canvasOpacity: value)),
      ),
      surfaceColorButton(true),
      if (!isWindowsDesktop)
        Text(
          l.mainWindowsFrostOnly,
          style: TextStyle(fontSize: 10, color: p.muted),
        ),
    ],
  );

  Widget componentMaterialSettings() => OutlinedButton.icon(
    key: const ValueKey('component-settings'),
    onPressed: () => settingsNavigator.currentState!.push(
      CanvasSettingsRoute<void>(
        builder: (_) => ComponentMaterialListPage(
          palette: p,
          entries: componentEntries,
          onChanged: (value) {
            if (!mounted) return;
            updateSurfaces(value);
            widget.onAppearanceCommit();
          },
        ),
      ),
    ),
    icon: Icon(Icons.tune_rounded, size: 16, color: p.accent),
    label: Text(l.mainComponentSettings, style: TextStyle(fontSize: 11)),
    style: OutlinedButton.styleFrom(
      splashFactory: NoSplash.splashFactory,
      minimumSize: const Size.fromHeight(38),
      side: BorderSide(color: p.line),
      shape: RoundedRectangleBorder(borderRadius: p.borderRadius(11)),
    ),
  );

  Map<String, String> get componentEntries => {
    'navigation': l.mainComponentNavigation,
    'search': l.mainComponentSearch,
    'hero': l.mainComponentHero,
    'quick-capture': l.mainComponentQuickCapture,
    'appearance': l.mainAppearance,
    'plugin-settings': l.mainPluginSettings,
    if (widget.workbench is ExternalPluginControl) ...{
      'io:permissions': l.pluginsIoTitle,
      'io:ServiceManager': l.mainServiceSettings,
      'io:ServiceRunManager': l.mainServiceRunSettings,
      'io:HttpTaskManager': l.mainHttpSettings,
      'io:CredentialManager': l.mainCredentialSettings,
      'io:EndpointManager': l.mainEndpointSettings,
    },
    if (widget.workbench is WorkbenchPluginControl)
      'plugin-tools': l.mainWorkbenchPlugin,
    if (widget.workbench is ExternalPluginControl)
      'plugin-library': l.mainExtensionPlugins,
    if (widget.workbench is WorkbenchProtectionBackup)
      'protection-backup': l.mainContentProtection,
    'daily': l.mainDaily,
    'music': l.mainMusic,
    'footer': l.mainComponentFooter,
    'empty': l.mainComponentEmpty,
    for (final page in WorkbenchPage.values.where(
      (p) => p != WorkbenchPage.overview,
    ))
      WorkbenchV1.summaryComponentId(page): l.mainPageSummary(
        workbenchLabels.page(page),
      ),
    for (final idea in ideas) 'card:${idea.id}': displayTitle(idea),
  };

  Future<void> openComponentSettings(String id, String title) async {
    if (p.overridesMaterials) {
      return;
    }
    final result = await settingsNavigator.currentState!
        .push<ComponentMaterial>(
          CanvasSettingsRoute<ComponentMaterial>(
            builder: (_) => ComponentMaterialPage(
              palette: p,
              id: id,
              title: title,
              entries: componentEntries,
              initial:
                  p.surfaces.components[id] ??
                  ComponentMaterial(
                    blur: p.surfaces.componentCustom
                        ? p.surfaces.componentBlur
                        : p.clear
                        ? 1
                        : p.liquid
                        ? 6
                        : 22,
                    opacity: p.surfaces.componentCustom
                        ? p.surfaces.componentOpacity
                        : p.clear
                        ? .10
                        : p.liquid
                        ? .18
                        : p.frostedOpacity,
                    color: p.surfaces.componentColor,
                  ),
            ),
          ),
        );
    if (!mounted || result == null || p.overridesMaterials) return;
    updateSurfaces(
      p.surfaces.copyWith(components: {...p.surfaces.components, id: result}),
    );
    widget.onAppearanceCommit();
  }

  Widget cardInteractions(Idea idea, Widget child, {Key? key}) =>
      ComponentContextMenu(
        key: key,
        actions: () => [
          ComponentMenuAction(
            label: l.mainView,
            icon: Icons.open_in_new,
            onSelected: () => openIdea(idea),
          ),
          ComponentMenuAction(
            label: idea.favorite
                ? l.mainUnfavoriteTooltip(displayTitle(idea))
                : l.mainFavoriteTooltip(displayTitle(idea)),
            icon: idea.favorite
                ? Icons.bookmark_remove_outlined
                : Icons.bookmark_add_outlined,
            enabled:
                (widget.workbench == null || widget.workbench!.writable) &&
                _pluginBusyOwner == null,
            onSelected: () {
              if (widget.workbench != null) {
                pluginChange(PluginAction.favorite, idea, flag: !idea.favorite);
              } else {
                refreshPage(() => idea.favorite = !idea.favorite);
                persist();
              }
            },
          ),
          if (!p.overridesMaterials)
            ComponentMenuAction(
              label: l.mainComponentSettings,
              icon: Icons.tune,
              enabled: widget.workbench == null || widget.workbench!.writable,
              onSelected: () =>
                  openComponentSettings('card:${idea.id}', displayTitle(idea)),
            ),
        ],
        child: SurfaceInteraction(
          borderRadius: p.borderRadius(20),
          child: child,
        ),
      );

  Future<void> openPluginSettings() async {
    final themes = ThemePluginScope.of(context);
    final backend = widget.workbench;
    if (backend == null) return;
    await settingsNavigator.currentState!.push<void>(
      CanvasSettingsRoute<void>(
        builder: (_) => PluginSettingsPage(
          backend: backend,
          onChanged: () {
            if (themes != null) unawaited(themes.refresh());
            if (mounted) setState(() => _queries.invalidate());
          },
        ),
      ),
    );
    if (themes != null) await themes.refresh();
    if (mounted) setState(() => _queries.invalidate());
  }

  Widget readOnlyNotice() => Glass(
    p: p,
    child: Padding(
      padding: const EdgeInsets.all(12),
      child: Wrap(
        alignment: WrapAlignment.spaceBetween,
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 12,
        runSpacing: 12,
        children: [
          Text(
            l.mainReadOnlySettings,
            style: TextStyle(color: p.ink, height: 1.5),
          ),
          TextButton.icon(
            key: const ValueKey('readonly-plugin-settings'),
            style: TextButton.styleFrom(splashFactory: NoSplash.splashFactory),
            onPressed: openPluginSettings,
            icon: const Icon(Icons.extension_outlined),
            label: Text(l.mainPluginSettings),
          ),
        ],
      ),
    ),
  );

  Widget appearance() {
    final backend = widget.workbench;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        appearanceControls(),
        if (backend is WorkbenchPluginControl ||
            backend is ExternalPluginControl ||
            backend is WorkbenchProtectionBackup) ...[
          const SizedBox(height: 20),
          Glass(
            p: p,
            componentId: 'plugin-settings',
            child: SettingsNavigationFeedback(
              child: ListTile(
                key: const ValueKey('plugin-settings-open'),
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: 20,
                  vertical: 12,
                ),
                leading: Icon(Icons.extension_outlined, color: p.accent),
                title: Text(
                  l.mainPluginSettings,
                  style: TextStyle(color: p.ink),
                ),
                subtitle: Text(
                  l.mainPluginSettingsSummary,
                  style: TextStyle(color: p.muted),
                ),
                trailing: Icon(Icons.chevron_right, color: p.muted),
                onTap: openPluginSettings,
              ),
            ),
          ),
        ],
      ],
    );
  }

  Widget languagePicker() {
    final locale = _UiLocaleScope.of(context);
    if (locale == null) return const SizedBox.shrink();
    return DropdownButtonFormField<String>(
      key: const ValueKey('language-picker'),
      initialValue: locale.value,
      isExpanded: true,
      decoration: InputDecoration(labelText: l.mainLanguage),
      items: [
        DropdownMenuItem(value: 'system', child: Text(l.mainLanguageSystem)),
        for (final entry in L10n.nativeNames.entries)
          DropdownMenuItem(value: entry.key, child: Text(entry.value)),
      ],
      onChanged: (value) {
        if (value != null) locale.onChanged(value);
      },
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
            key: const ValueKey('appearance-heading'),
            children: [
              SizedBox.square(
                dimension: 40,
                child: Center(
                  child: Icon(
                    Icons.tune_rounded,
                    key: const ValueKey('appearance-heading-icon'),
                    size: 16,
                    color: p.ink,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  l.mainAppearance,
                  key: const ValueKey('appearance-heading-title'),
                  textAlign: TextAlign.start,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: p.ink,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 100),
                child: Text(
                  l.mainMakeYours,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textAlign: TextAlign.end,
                  style: TextStyle(
                    fontSize: 7,
                    letterSpacing: 1,
                    color: p.muted,
                  ),
                ),
              ),
              if (!compactSettingsOpen)
                IconButton(
                  key: const ValueKey('settings-expand'),
                  tooltip: l.mainExpandSettings,
                  style: IconButton.styleFrom(
                    fixedSize: const Size.square(40),
                    minimumSize: const Size.square(40),
                    padding: EdgeInsets.zero,
                    visualDensity: VisualDensity.standard,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                  ),
                  onPressed: () {
                    FocusManager.instance.primaryFocus?.unfocus();
                    setState(() => compactSettingsOpen = true);
                  },
                  icon: Icon(Icons.open_in_full, size: 16, color: p.muted),
                ),
            ],
          ),
          const SizedBox(height: 23),
          VisualStylePicker(
            palette: p,
            onChanged: (style) {
              updateSurfaces(p.surfaces.copyWith(visualStyle: style));
              widget.onAppearanceCommit();
            },
          ),
          if (p.visualStyle.supportsDepth) ...[
            const SizedBox(height: 16),
            StyleDepthSlider(
              key: const ValueKey('style-depth'),
              palette: p,
              value: p.surfaces.styleDepth,
              onChanged: (v) =>
                  updateSurfaces(p.surfaces.copyWith(styleDepth: v)),
              onChangeEnd: (_) => widget.onAppearanceCommit(),
              onReset: () {
                updateSurfaces(p.surfaces.copyWith(styleDepth: 1));
                widget.onAppearanceCommit();
              },
            ),
          ],
          const SizedBox(height: 16),
          languagePicker(),
          const SizedBox(height: 12),
          ListTile(
            key: const ValueKey('font-settings'),
            contentPadding: EdgeInsets.zero,
            leading: Icon(Icons.font_download_outlined, color: p.ink),
            title: Text(l.mainFontSettings, style: TextStyle(color: p.ink)),
            trailing: Icon(Icons.chevron_right, color: p.muted),
            onTap: () => settingsNavigator.currentState!.push(
              CanvasSettingsRoute<void>(
                builder: (_) => const FontSettingsPage(),
              ),
            ),
          ),
          const SizedBox(height: 18),
          if (!p.overridesMaterials) ...[
            label(l.mainGlassTexture),
            const SizedBox(height: 10),
            Row(
              children: [
                Expanded(
                  child: modeOption(
                    GlassMode.frosted,
                    l.mainFrosted,
                    Icons.blur_on_rounded,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: modeOption(
                    GlassMode.clear,
                    l.mainCrystal,
                    Icons.water_drop_outlined,
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: modeOption(
                    GlassMode.liquid,
                    l.mainLiquidGlass,
                    Icons.lens_blur,
                  ),
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
                        Expanded(
                          child: Text(
                            l.mainFrostOpacity,
                            style: TextStyle(fontSize: 10, color: p.muted),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text(
                          '${(p.frostedOpacity * 100).round()}%',
                          style: TextStyle(fontSize: 11, color: p.accent),
                        ),
                      ],
                    ),
                    AnimatedSliderStyle(
                      palette: p,
                      flatThumbRadius: 6,
                      flatTrackHeight: 3,
                      overlayRadius: 12,
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
                        Expanded(
                          child: Text(
                            l.mainLightOpacity,
                            style: TextStyle(fontSize: 9, color: p.muted),
                          ),
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Text(
                            l.mainSolidOpacity,
                            textAlign: TextAlign.end,
                            style: TextStyle(fontSize: 9, color: p.muted),
                          ),
                        ),
                      ],
                    ),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 16),
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
                                      ? l.mainLiquidGlass
                                      : p.clear
                                      ? l.mainCrystal
                                      : l.mainFrosted,
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
                  ? l.mainLiquidDetail
                  : p.clear
                  ? l.mainCrystalDetail
                  : l.mainFrostDetail,
              style: TextStyle(fontSize: 9, color: p.muted),
            ),
          ],
          const SizedBox(height: 22),
          label(l.mainThemeTone),
          const SizedBox(height: 12),
          Row(
            children: StudioTheme.values
                .where(
                  (theme) => p.uiTheme == null || theme != StudioTheme.custom,
                )
                .map((theme) => Expanded(child: themeOption(theme)))
                .toList(),
          ),
          const SizedBox(height: 12),
          if (p.uiTheme == null) ...[
            OutlinedButton.icon(
              key: const ValueKey('theme-color-compass'),
              onPressed: chooseThemeColor,
              icon: Icon(Icons.palette_outlined, color: p.accent, size: 16),
              label: Text(l.mainThemeCompass, style: TextStyle(fontSize: 11)),
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
                        ? l.mainDefaultGlobalColor
                        : l.mainGlobalColor(
                            '#${p.themeColor!.toARGB32().toRadixString(16).substring(2).toUpperCase()}',
                          ),
                    style: TextStyle(color: p.muted, fontSize: 10),
                  ),
                ),
                Flexible(
                  child: TextButton(
                    key: const ValueKey('theme-color-reset'),
                    onPressed: p.themeColor == null
                        ? null
                        : () {
                            widget.onThemeColor(null);
                            widget.onAppearanceCommit();
                          },
                    child: Text(l.mainRestoreDefault),
                  ),
                ),
              ],
            ),
          ],
          if (p.isCustom && p.uiTheme == null) ...[
            const SizedBox(height: 12),
            TextButton(
              key: const ValueKey('custom-tone-toggle'),
              onPressed: () => setState(() => showCustomTone = !showCustomTone),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      showCustomTone
                          ? l.mainHideCustomTone
                          : l.mainAdjustCustomTone,
                    ),
                  ),
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
                            Expanded(child: label(l.mainThemeGrayscale)),
                            label('${(p.grayscale * 100).round()}%'),
                          ],
                        ),
                        AnimatedSliderStyle(
                          palette: p,
                          child: Slider(
                            key: const ValueKey('theme-grayscale'),
                            value: p.grayscale,
                            divisions: 100,
                            label: '${(p.grayscale * 100).round()}%',
                            onChanged: widget.onGrayscale,
                            onChangeEnd: (_) => widget.onAppearanceCommit(),
                          ),
                        ),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            label(l.mainOriginalColors),
                            label(l.mainMonochrome),
                          ],
                        ),
                        Divider(height: 28, color: p.line),
                        Row(
                          children: [
                            Expanded(child: label(l.mainCustomLightness)),
                            label('${(p.lightness * 100).round()}%'),
                          ],
                        ),
                        AnimatedSliderStyle(
                          palette: p,
                          child: Slider(
                            key: const ValueKey('theme-lightness'),
                            value: p.lightness,
                            divisions: 100,
                            label: '${(p.lightness * 100).round()}%',
                            onChanged: widget.onLightness,
                            onChangeEnd: (_) => widget.onAppearanceCommit(),
                          ),
                        ),
                        Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            label(l.mainDeepBlack),
                            label(l.mainBrightWhite),
                          ],
                        ),
                      ],
                    )
                  : const SizedBox.shrink(),
            ),
          ],
          if (!p.overridesMaterials) ...[
            Divider(height: 28, color: p.line),
            Row(
              children: [
                Expanded(child: label(l.mainCornerRadius)),
                label('${p.cornerRadius.round()} / 32'),
              ],
            ),
            AnimatedSliderStyle(
              palette: p,
              child: Slider(
                key: const ValueKey('corner-radius'),
                value: p.cornerRadius,
                min: 0,
                max: 32,
                divisions: 32,
                label: '${p.cornerRadius.round()}',
                onChanged: widget.onRadius,
                onChangeEnd: (_) => widget.onAppearanceCommit(),
              ),
            ),
            Text(
              l.mainSquareCorners,
              style: TextStyle(fontSize: 9, color: p.muted),
            ),
          ],
          if (widget.desktopCaption) ...[
            const SizedBox(height: 16),
            Row(
              children: [
                Expanded(child: label(l.mainWindowRadius)),
                label('${p.windowRadius.round()} / 32'),
              ],
            ),
            AnimatedSliderStyle(
              palette: p,
              child: Slider(
                key: const ValueKey('window-radius'),
                value: p.windowRadius,
                max: 32,
                divisions: 32,
                label: '${p.windowRadius.round()}',
                onChanged: widget.onWindowRadius,
                onChangeEnd: (_) => widget.onAppearanceCommit(),
              ),
            ),
            Text(
              l.mainWindowRadiusDetail,
              style: TextStyle(fontSize: 9, color: p.muted),
            ),
          ],
          if (!p.overridesMaterials) ...[
            Divider(height: 24, color: p.line),
            label(l.mainBackgroundCanvas),
            const SizedBox(height: 10),
            LayoutBuilder(
              builder: (_, constraints) => Wrap(
                spacing: 12,
                runSpacing: 12,
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
            NeumorphicSwitchListTile(
              palette: p,
              key: const ValueKey('canvas-liquid-toggle'),
              contentPadding: EdgeInsets.zero,
              title: Text(l.mainLiquidEffect, style: TextStyle(fontSize: 12)),
              subtitle: Text(
                l.mainLiquidAllCanvases,
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
                            message: [
                              l.mainFollowTheme,
                              l.mainLavender,
                              l.mainSage,
                              l.mainWarmSand,
                            ][index],
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
                                    p.customColor == null &&
                                        p.solidTint == index
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
                              ? l.mainCustomCompass
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
                        spacing: 12,
                        runSpacing: 12,
                        children: [
                          OutlinedButton.icon(
                            key: const ValueKey('texture-file'),
                            onPressed: importing ? null : () => chooseTexture(),
                            icon: const Icon(
                              Icons.upload_file_outlined,
                              size: 15,
                            ),
                            label: Text(
                              l.mainLocalMedia,
                              style: TextStyle(fontSize: 10),
                            ),
                          ),
                          OutlinedButton.icon(
                            key: const ValueKey('texture-link'),
                            onPressed: importing
                                ? null
                                : () => chooseTexture(online: true),
                            icon: const Icon(Icons.link, size: 15),
                            label: Text(
                              l.mainOnlineMedia,
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
                              Expanded(child: label(l.mainBackgroundSound)),
                              NeumorphicSwitch(
                                palette: p,
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
                                  p.mediaPlaying ? l.mainPause : l.mainPlay,
                                  style: const TextStyle(fontSize: 10),
                                ),
                              ),
                            Expanded(
                              child: TextButton(
                                onPressed: () {
                                  setState(() => mediaError = null);
                                  widget.onTexture(null);
                                },
                                child: Text(
                                  l.mainBuiltinTexture,
                                  style: TextStyle(fontSize: 10),
                                ),
                              ),
                            ),
                          ],
                        ),
                      ] else
                        Text(
                          l.mainMediaLimits,
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
              BackgroundMode.ambient => l.mainAmbientDetail,
              BackgroundMode.solid => l.mainSolidDetail,
              BackgroundMode.texture => l.mainTextureDetail,
              BackgroundMode.transparent =>
                !kIsWeb && defaultTargetPlatform == TargetPlatform.android
                    ? l.mainOpaqueFallback
                    : l.mainTransparentDetail,
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
                Expanded(
                  child: Text(
                    l.mainAutosaveNotice,
                    style: TextStyle(fontSize: 9, color: p.muted),
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    ),
  );

  Widget label(String text) =>
      Text(text, style: TextStyle(fontSize: 10, color: p.muted));

  Widget backgroundOption(BackgroundMode mode) {
    final (title, icon) = switch (mode) {
      BackgroundMode.ambient => (l.mainDefaultCanvas, Icons.gradient_rounded),
      BackgroundMode.solid => (l.mainSolidCanvas, Icons.circle_outlined),
      BackgroundMode.texture => (l.mainTextureCanvas, Icons.grain_rounded),
      BackgroundMode.transparent => (
        l.mainTransparentCanvas,
        Icons.layers_clear_outlined,
      ),
    };
    final selected = p.backdrop == mode;
    return Semantics(
      button: true,
      selected: selected,
      child: InkWell(
        key: ValueKey('background-${mode.name}'),
        onTap: () => widget.onBackground(mode),
        borderRadius: p.borderRadius(10),
        child: NeumorphicSurface(
          palette: p,
          depth: selected ? -0.9 : .6,
          borderRadius: p.borderRadius(10),
          child: AnimatedContainer(
            duration: motionDuration(context, 200),
            constraints: const BoxConstraints(minHeight: 38),
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
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
                Flexible(
                  child: Text(
                    title,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 10,
                      color: selected ? p.accent : p.muted,
                    ),
                  ),
                ),
              ],
            ),
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
        child: NeumorphicSurface(
          palette: p,
          depth: selected ? -0.9 : .6,
          borderRadius: p.borderRadius(10),
          child: AnimatedContainer(
            duration: motionDuration(context, 220),
            constraints: const BoxConstraints(minHeight: 56),
            padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 8),
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
              mainAxisSize: MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(icon, size: 15, color: selected ? p.accent : p.muted),
                const SizedBox(height: 5),
                Text(
                  title,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 11,
                    color: selected ? p.accent : p.muted,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget themeOption(StudioTheme theme) {
    final selected = p.uiTheme == null
        ? p.theme == theme
        : (p.dark ? StudioTheme.dark : StudioTheme.white) == theme;
    final color = switch (theme) {
      StudioTheme.white => const Color(0xFFFAFAFC),
      StudioTheme.custom => const Color(0xFFCFD1DA),
      StudioTheme.dark => const Color(0xFF302E3E),
    };
    final title = switch (theme) {
      StudioTheme.white => l.mainWhiteTheme,
      StudioTheme.custom => l.mainCustomTheme,
      StudioTheme.dark => l.mainDarkTheme,
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
              NeumorphicSurface(
                palette: p,
                depth: selected ? -1 : .65,
                borderRadius: BorderRadius.circular(24),
                child: AnimatedContainer(
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
                l.mainDaily,
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
            title: l.mainDailyWater,
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
            title: l.mainDailyIdea,
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
            title: l.mainDailyExplore,
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
          Text(
            l.mainSlowProgress,
            style: TextStyle(fontSize: 9, color: p.muted),
          ),
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
          l.mainCuriosity,
          style: TextStyle(color: p.muted, fontSize: 12, height: 1.9),
        ),
        const SizedBox(height: 12),
        Text(
          l.mainStayCurious,
          style: TextStyle(fontSize: 7, color: p.muted, letterSpacing: 1.4),
        ),
      ],
    ),
  );

  bool _acceptEditorResult(Idea result, WorkbenchBackend? backend) {
    if (!mounted || !identical(widget.workbench, backend)) return false;
    final accepted = _acceptContentResult(
      result,
      remove: result.contentDeleted,
    );
    if (!accepted && backend is WorkbenchEditorSupport) {
      unawaited(_refreshAfterEditorClose(backend));
    }
    return accepted;
  }

  Future<WorkbenchEditorSession?> _openEditor(String id, bool create) async {
    final backend = widget.workbench;
    if (backend case WorkbenchEditorSupport editor) {
      return editor.openEditor(id, create: create);
    }
    return null;
  }

  Future<WorkbenchEditorSession> _openConfirmedVersionedEditor(
    WorkbenchBackend backend,
    Idea confirmed,
  ) async {
    bool current() => mounted && identical(widget.workbench, backend);
    if (!current() ||
        !backend.writable ||
        backend is! WorkbenchVersionedContent ||
        backend is! WorkbenchVersionedEditorSupport ||
        backend is! WorkbenchMixedContent ||
        confirmed.contentRevision == null ||
        confirmed.contentDeleted) {
      throw StateError('Confirmed editor is no longer available');
    }
    final record = await (backend as WorkbenchVersionedContent).versionedContent
        .read(confirmed.id);
    if (!current() ||
        record.revision != confirmed.contentRevision ||
        record.formatVersion != 2 ||
        record.deleted) {
      throw StateError('Confirmed editor baseline changed');
    }
    final view = await (backend as WorkbenchMixedContent).workspaceRecord(
      record,
    );
    if (!current() || view.contentRevision != confirmed.contentRevision) {
      throw StateError('Confirmed editor presentation changed');
    }
    final session = await (backend as WorkbenchVersionedEditorSupport)
        .openVersionedEditor(
          confirmed.id,
          expectedRevision: confirmed.contentRevision,
        );
    if (!current()) {
      await session.close();
      throw StateError('Editor workspace changed');
    }
    return VersionedWorkbenchEditorAdapter(
      session,
      view.versioned!,
      (backend as WorkbenchMixedContent).workspaceRecord,
      isCurrent: current,
      reopen: (next) => _openConfirmedVersionedEditor(backend, next),
    );
  }

  void _editorOpenError(Object error) {
    if (mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(l.mainEditorUnavailable)));
    }
  }

  Future<void> _refreshAfterEditorClose(WorkbenchBackend? backend) async {
    if (backend case WorkbenchEditorSupport editor) {
      try {
        for (var attempt = 0; attempt < 3; attempt++) {
          final generation = _contentGeneration;
          final current = await editor.refreshEditorContent();
          if (!mounted || !identical(widget.workbench, backend)) return;
          final currentIds = current.map((item) => item.id).toSet();
          final WorkbenchContentRevisionSource? source =
              backend is WorkbenchContentRevisionSource
              ? backend as WorkbenchContentRevisionSource
              : null;
          if (generation != _contentGeneration ||
              (source != null &&
                  source.knownContentIds().any((id) {
                    final deleted = source.knownContentDeleted(id);
                    return deleted == true
                        ? currentIds.contains(id)
                        : !currentIds.contains(id);
                  })) ||
              current.any((item) {
                final known = _knownContentRevisions[item.id];
                final latest = source?.knownContentRevision(item.id);
                final minimum =
                    latest != null && (known == null || latest > known)
                    ? latest
                    : known;
                return minimum != null &&
                    item.contentRevision != null &&
                    item.contentRevision! < minimum;
              })) {
            continue;
          }
          setState(() {
            ideas
              ..clear()
              ..addAll(current);
            for (final item in current) {
              if (item.contentRevision case final version?) {
                _knownContentRevisions[item.id] = version;
              }
            }
            _contentGeneration++;
          });
          persist();
          return;
        }
        throw StateError('Current content changed during refresh');
      } catch (_) {
        if (mounted && identical(widget.workbench, backend)) {
          ScaffoldMessenger.of(
            context,
          ).showSnackBar(SnackBar(content: Text(l.mainEditorClosedUnknown)));
        }
      }
    }
  }

  Future<void> createIdea() async {
    final backend = widget.workbench;
    final target = Idea.nextId();
    WorkbenchEditorSession? editor;
    try {
      editor = await _openEditor(target, true);
    } catch (error) {
      _editorOpenError(error);
      return;
    }
    if (!mounted || !identical(widget.workbench, backend)) {
      await editor?.close();
      return;
    }
    final result = await showStudioDialog<Idea>(
      context: context,
      builder: (_) => NewIdeaDialog(
        targetId: target,
        editor: editor,
        isCurrent: () => mounted && identical(widget.workbench, backend),
        initialFavorite: section == WorkbenchPage.favorites,
        plugin: widget.workbench?.studio,
        initialCategory: switch (section) {
          WorkbenchPage.projects => '进行中',
          WorkbenchPage.laboratory => '实验',
          _ => '灵感',
        },
      ),
    );
    if (!mounted || !identical(widget.workbench, backend)) return;
    if (result == null) {
      if (editor != null) await _refreshAfterEditorClose(backend);
      return;
    }
    if (editor != null) {
      if (!_acceptEditorResult(result, backend)) return;
      setState(() {
        filter = GeneralFilter.all;
        query = '';
        search.clear();
      });
      return;
    }
    if (backend != null) {
      if (section == WorkbenchPage.favorites) result.favorite = true;
      final saved = await pluginChange(PluginAction.create, result);
      if (saved != null && mounted && identical(widget.workbench, backend)) {
        setState(() {
          filter = GeneralFilter.all;
          query = '';
          search.clear();
        });
      }
      return;
    }
    setState(() {
      ideas.insert(0, result);
      if (section == WorkbenchPage.favorites) result.favorite = true;
      filter = GeneralFilter.all;
      query = '';
      search.clear();
    });
    persist();
  }

  void _dismissDraftHandoffRoute() {
    final route = _draftHandoffRoute;
    _draftHandoffRoute = null;
    if (route == null) return;
    // Navigation can be locked while Studio updates/disposes. Remove only our
    // owned route after this frame, never pop an unrelated new-workspace page.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (route.isActive) route.navigator?.removeRoute(route);
    });
  }

  Future<void> reviewDraftHandoffRecovery() async {
    final backend = widget.workbench;
    if (backend is! WorkbenchEditorDraftHandoffProposalSupport) return;
    bool current() => mounted && identical(widget.workbench, backend);
    bool canWrite() => current() && backend!.writable;
    final coordinator = _draftHandoffRecovery ??= EditorDraftHandoffCoordinator(
      (backend as WorkbenchEditorDraftHandoffProposalSupport)
          .editorDraftHandoffProposals,
      isCurrent: current,
      canWrite: canWrite,
      beforeAction: (record, action) async {
        if (backend is! WorkbenchPluginControl) {
          throw StateError('Current plugin state is unavailable');
        }
        final plugin = await (backend as WorkbenchPluginControl).pluginState();
        if (!canWrite() || !plugin.writable) {
          throw StateError('Draft recovery workspace permission changed');
        }
        // Cancellation/retirement release host-owned draft state; they do not
        // require executing an installed plugin or replaying business content.
        if (action == EditorDraftHandoffAction.complete) {
          if (!plugin.enabled ||
              !plugin.approved ||
              !plugin.available ||
              backend is! WorkbenchVersionedContent) {
            throw StateError('Draft recovery source plugin is unavailable');
          }
          final source = await (backend as WorkbenchVersionedContent)
              .versionedContent
              .read(record.summary.cardId);
          if (!canWrite() ||
              source.deleted ||
              source.revision !=
                  record.proposal.handoff.request.sourceRevision) {
            throw StateError('Draft recovery source changed');
          }
        }
      },
    );
    if (identical(_draftHandoffDialogOwner, coordinator)) return;
    _draftHandoffDialogOwner = coordinator;
    final route = DialogRoute<void>(
      context: context,
      builder: (_) => EditorDraftHandoffRecoveryDialog(
        coordinator: coordinator,
        isCurrent: current,
        canWrite: canWrite,
      ),
    );
    _draftHandoffRoute = route;
    try {
      await Navigator.of(context, rootNavigator: true).push(route);
    } finally {
      if (identical(_draftHandoffRoute, route)) _draftHandoffRoute = null;
      if (identical(_draftHandoffDialogOwner, coordinator)) {
        _draftHandoffDialogOwner = null;
      }
    }
  }

  Future<void> reviewDraftImportRecovery() async {
    final backend = widget.workbench;
    if (backend is! WorkbenchEditorDraftImportSupport) return;
    bool current() => mounted && identical(widget.workbench, backend);
    final coordinator = EditorDraftImportDecisionCoordinator(
      (backend as WorkbenchEditorDraftImportSupport).editorDraftImports,
      isCurrent: current,
    );
    try {
      await showDialog<void>(
        context: context,
        builder: (_) => EditorDraftImportRecoveryDialog(
          coordinator: coordinator,
          writable: backend!.writable,
          isCurrent: current,
        ),
      );
    } finally {
      coordinator.dispose();
    }
  }

  Future<void> reviewEditorRecovery() async {
    final backend = widget.workbench;
    if (backend is! WorkbenchEditorRecovery ||
        backend is! WorkbenchVersionedContent ||
        backend is! WorkbenchMixedContent) {
      return;
    }
    await showDialog<void>(
      context: context,
      builder: (_) => EditorRecoveryDialog(
        control: backend as WorkbenchEditorRecovery,
        isCurrent: () => mounted && identical(widget.workbench, backend),
        writable: backend!.writable,
        presentCurrent: (id) async {
          if (!mounted || !identical(widget.workbench, backend)) {
            throw StateError('Workspace changed');
          }
          final record = await (backend as WorkbenchVersionedContent)
              .versionedContent
              .read(id);
          final current = await (backend as WorkbenchMixedContent)
              .workspaceRecord(record);
          if (!mounted || !identical(widget.workbench, backend)) {
            throw StateError('Workspace changed');
          }
          if (!_acceptContentResult(current, remove: current.contentDeleted)) {
            throw StateError('Current view changed');
          }
        },
      ),
    );
  }

  Future<void> openIdea(Idea idea) async {
    final backend = widget.workbench;
    void rebindVersionedDialog(
      BuildContext dialogContext,
      StateSetter refresh,
    ) {
      if (!mounted ||
          !dialogContext.mounted ||
          !identical(widget.workbench, backend)) {
        return;
      }
      for (final current in ideas) {
        if (current.id != idea.id ||
            current.versioned?.id != idea.id ||
            (current.contentOwner != null &&
                !identical(current.contentOwner, backend)) ||
            (idea.contentRevision != null &&
                current.contentRevision != null &&
                current.contentRevision! < idea.contentRevision!)) {
          continue;
        }
        refresh(() => idea = current);
        return;
      }
    }

    final action = await showStudioDialog<String>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (_, refresh) => StudioDialog(
          icon: idea.icon,
          title: displayTitle(idea),
          subtitle: l.mainIdeaDetails,
          content: SizedBox(
            width: 390,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    categoryLabel(idea.category),
                    style: TextStyle(color: p.accent, fontSize: 12),
                  ),
                  const SizedBox(height: 18),
                  IdeaMarkdown(
                    data: displayDescription(idea),
                    attachments: idea.attachments,
                  ),
                  if (idea.attachments.isNotEmpty) ...[
                    const SizedBox(height: 18),
                    Text(
                      l.mainAttachmentCount(idea.attachments.length),
                      style: TextStyle(color: p.accent),
                    ),
                    ...idea.attachments.map(
                      (item) => AttachmentTile(attachment: item),
                    ),
                  ],
                  if (idea.category == '实验') ...[
                    const SizedBox(height: 18),
                    Text(l.mainHypothesis, style: TextStyle(color: p.accent)),
                    SelectableText(
                      idea.hypothesis.isEmpty
                          ? l.mainNoHypothesis
                          : idea.hypothesis,
                    ),
                    const SizedBox(height: 12),
                    Text(l.mainObservations, style: TextStyle(color: p.accent)),
                    SelectableText(
                      idea.conclusion.isEmpty
                          ? l.mainAwaitDiscovery
                          : idea.conclusion,
                    ),
                  ],
                  if (idea.versioned != null) ...[
                    const SizedBox(height: 18),
                    DropdownButtonFormField<String>(
                      key: ValueKey(
                        'versioned-category-${idea.id}-${idea.contentRevision}',
                      ),
                      initialValue: idea.category,
                      decoration: InputDecoration(
                        labelText: l.mainCategoryPrompt,
                      ),
                      items: ['灵感', '进行中', '实验']
                          .map(
                            (value) => DropdownMenuItem(
                              value: value,
                              child: Text(
                                value == '灵感'
                                    ? l.mainPageInbox
                                    : value == '进行中'
                                    ? l.mainPageProjects
                                    : l.mainPageLaboratory,
                              ),
                            ),
                          )
                          .toList(),
                      onChanged: backend?.writable != true
                          ? null
                          : (value) async {
                              if (!mounted ||
                                  !identical(widget.workbench, backend) ||
                                  value == null ||
                                  value == idea.category) {
                                return;
                              }
                              try {
                                final updated = await versionedChange(
                                  idea,
                                  CardEditCommand.setCategory(
                                    value,
                                    value == '进行中'
                                        ? '计划中'
                                        : value == '实验'
                                        ? '待验证'
                                        : '待整理',
                                  ),
                                );
                                if (mounted &&
                                    dialogContext.mounted &&
                                    identical(widget.workbench, backend)) {
                                  refresh(() => idea = updated);
                                }
                              } catch (error) {
                                if (dialogContext.mounted &&
                                    (error is VersionedMutationNoCommit ||
                                        error
                                            is VersionedMutationNotSubmitted)) {
                                  rebindVersionedDialog(dialogContext, refresh);
                                }
                              }
                            },
                    ),
                    VersionedTaskPanel(
                      key: ValueKey('versioned-tasks-${idea.id}'),
                      view: idea.versioned!,
                      writable: backend?.writable == true,
                      busy: _pluginBusyOwner != null,
                      onCommand: (command) async {
                        if (!mounted || !identical(widget.workbench, backend)) {
                          throw const VersionedMutationNotSubmitted();
                        }
                        try {
                          final updated = await versionedChange(idea, command);
                          if (mounted &&
                              dialogContext.mounted &&
                              identical(widget.workbench, backend)) {
                            refresh(() => idea = updated);
                          }
                        } catch (error) {
                          if (dialogContext.mounted &&
                              (error is VersionedMutationNoCommit ||
                                  error is VersionedMutationNotSubmitted)) {
                            rebindVersionedDialog(dialogContext, refresh);
                          }
                          rethrow;
                        }
                      },
                    ),
                  ],
                  if (idea.todos.isNotEmpty) ...[
                    const SizedBox(height: 22),
                    Text(
                      l.mainProgress(
                        idea.legacyCompletedCount,
                        idea.todos.length,
                      ),
                      style: TextStyle(color: p.accent, fontSize: 11),
                    ),
                    const SizedBox(height: 8),
                    ...idea.todos.map(
                      (todo) => LittleTask(
                        title: displayTodo(idea, todo),
                        done: idea.completed.contains(todo),
                        onChanged: (done) async {
                          if (idea.todos
                                      .where((value) => value == todo)
                                      .length >
                                  1 &&
                              !await confirmLegacyTodoChange(
                                l.mainLegacyTodoGroup(todo),
                              )) {
                            return;
                          }
                          if (!mounted ||
                              !dialogContext.mounted ||
                              !identical(widget.workbench, backend)) {
                            return;
                          }
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
                l.mainDelete,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ),
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, 'edit'),
              child: Text(l.mainEdit),
            ),
            FilledButton.tonal(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(l.mainDone),
            ),
          ],
        ),
      ),
    );
    if (!mounted || !identical(widget.workbench, backend)) return;
    if (action == 'edit') {
      WorkbenchEditorSession? editor;
      try {
        if (idea.versioned != null) {
          if (backend is! WorkbenchVersionedContent ||
              backend is! WorkbenchVersionedEditorSupport ||
              backend is! WorkbenchMixedContent ||
              !backend!.writable) {
            throw StateError('Versioned editor unavailable');
          }
          final record = await (backend as WorkbenchVersionedContent)
              .versionedContent
              .read(idea.id);
          idea = await (backend as WorkbenchMixedContent).workspaceRecord(
            record,
          );
          final session = await (backend as WorkbenchVersionedEditorSupport)
              .openVersionedEditor(
                idea.id,
                expectedRevision: idea.contentRevision,
              );
          editor = VersionedWorkbenchEditorAdapter(
            session,
            idea.versioned!,
            (backend as WorkbenchMixedContent).workspaceRecord,
            isCurrent: () => mounted && identical(widget.workbench, backend),
            reopen: (confirmed) =>
                _openConfirmedVersionedEditor(backend, confirmed),
          );
        } else {
          editor = await _openEditor(idea.id, false);
        }
      } catch (error) {
        _editorOpenError(error);
        return;
      }
      if (!mounted || !identical(widget.workbench, backend)) {
        await editor?.close();
        return;
      }
      final edited = await showStudioDialog<Idea>(
        context: context,
        builder: (_) => NewIdeaDialog(
          initialIdea: idea,
          editor: editor,
          isCurrent: () => mounted && identical(widget.workbench, backend),
          plugin: widget.workbench?.studio,
        ),
      );
      if (!mounted || !identical(widget.workbench, backend)) return;
      if (edited == null) {
        if (editor != null) await _refreshAfterEditorClose(backend);
        return;
      }
      if (editor != null) {
        _acceptEditorResult(edited, backend);
        return;
      }
      if (backend != null) {
        await pluginChange(PluginAction.edit, edited);
        return;
      }
      setState(() {
        final index = ideas.indexWhere((item) => item.id == idea.id);
        if (index >= 0) ideas[index] = edited;
      });
      persist();
    } else if (action == 'delete') {
      if (idea.versioned != null) {
        await pluginChange(PluginAction.delete, idea);
        return; // Typed success, including original-operation retries, owns Undo.
      }
      final index = ideas.indexOf(idea);
      if (backend != null) {
        final deleted = await pluginChange(PluginAction.delete, idea);
        if (deleted == null ||
            !mounted ||
            !identical(widget.workbench, backend) ||
            (deleted.contentRevision != null && !deleted.contentDeleted)) {
          return;
        }
        idea = deleted;
      } else {
        setState(() => ideas.remove(idea));
        persist();
      }
      ScaffoldMessenger.of(context).hideCurrentSnackBar();
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(l.mainDeleted(displayTitle(idea))),
          duration: const Duration(seconds: 8),
          action: SnackBarAction(
            label: l.mainUndo,
            onPressed: () {
              if (!mounted ||
                  !identical(widget.workbench, backend) ||
                  ideas.any((item) => item.id == idea.id)) {
                return;
              }
              if (backend != null) {
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
          NeumorphicSurface(
            palette: AppearanceScope.of(context),
            depth: done ? -0.7 : 0.45,
            borderRadius: BorderRadius.circular(8),
            child: SoftSwap(
              child: Icon(
                done ? Icons.check_circle_rounded : Icons.circle_outlined,
                key: ValueKey(done),
                color: Theme.of(
                  context,
                ).colorScheme.primary.withValues(alpha: done ? .9 : .4),
                size: 16,
              ),
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
    this.isCurrent,
  });
  final Idea? initialIdea;
  final String? targetId;
  final bool initialFavorite;
  final bool Function()? isCurrent;
  final WorkbenchEditorSession? editor;
  final String initialCategory;
  final Future<PastedContent> Function()? readClipboard;
  final Future<IdeaAttachment> Function(XFile)? importAttachment;
  final StudioBackend? plugin;
  @override
  State<NewIdeaDialog> createState() => _NewIdeaDialogState();
}

class _NewIdeaDialogState extends State<NewIdeaDialog> {
  AppLocalizations get l => L10n.of(context);
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
  WorkbenchEditorSession? _editor, _preparedSuccessor;
  Idea? _baseline, _confirmedAwaitingContinuation;
  int _editGeneration = 0;
  int? _submittedGeneration;
  bool _hasUnsavedSuccessor = false;
  final _lastFieldValues = <TextEditingController, TextEditingValue>{};

  void _observeDraft(TextEditingController controller) {
    final previous = _lastFieldValues[controller];
    final next = controller.value;
    _lastFieldValues[controller] = next;
    // Selection, affinity and direction are part of the user's live draft too.
    // A late S1 receipt must not close a view whose raw editing state changed.
    if (previous != next) {
      setState(() {
        _editGeneration++;
        if (controller == title && next.text.trim().isNotEmpty) invalid = false;
      });
    }
  }

  Future<void> _continueDraft() async {
    if (!(widget.isCurrent?.call() ?? true)) {
      final stale = _preparedSuccessor;
      _preparedSuccessor = null;
      await stale?.close();
      throw StateError('Editor workspace changed');
    }
    final confirmed = _confirmedAwaitingContinuation!;
    final previous = _editor;
    var next = _preparedSuccessor;
    if (next == null) {
      if (previous is WorkbenchEditorContinuation) {
        next = await (previous as WorkbenchEditorContinuation)
            .continueAfterCommit(confirmed);
      } else if (previous != null) {
        throw StateError('Editor cannot continue a confirmed draft');
      }
    }
    if (!mounted || !(widget.isCurrent?.call() ?? true)) {
      _preparedSuccessor = null;
      await next?.close();
      throw StateError('Editor workspace changed');
    }
    if (next != null && next.targetId != _targetId) {
      _preparedSuccessor = null;
      await next.close();
      throw StateError('Successor editor changed target');
    }
    // Opening a successor retires the old editor. Keep the new session even
    // if a live attachment alias needs correction before applying the rebase.
    _preparedSuccessor = next;
    final rebased = rebaseEditorAttachments(
      submitted: _frozenDraft!.attachments,
      confirmed: confirmed.attachments,
      selected: attachments,
      description: description.value,
    );
    _lastFieldValues[description] = rebased.description;
    description.value = rebased.description;
    setState(() {
      attachments
        ..clear()
        ..addAll(rebased.attachments);
      _editor = next;
      _preparedSuccessor = null;
      _baseline = confirmed;
      _confirmedAwaitingContinuation = null;
      _frozenDraft = null;
      _frozenFields = null;
      _submittedGeneration = null;
      _submitFrozen = false;
      _hasUnsavedSuccessor = true;
      saveError = '';
    });
  }

  int _pasteSequence = 0;
  StudioBackend? get _plugin => _editor?.studio ?? widget.plugin;
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
    await _editor?.recordPaste(
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
    if (target.text != before) throw StateError(l.mainPasteChanged);
    target.value = TextEditingValue(
      text: after,
      selection: TextSelection.collapsed(offset: start + inserted.length),
    );
  }

  Future<void> _save() async {
    if (importing || saving) return;
    if (!(widget.isCurrent?.call() ?? true)) {
      setState(() => saveError = l.mainEditorUnavailable);
      return;
    }
    if (_confirmedAwaitingContinuation != null) {
      setState(() {
        saving = true;
        saveError = '';
      });
      try {
        await _continueDraft();
      } catch (_) {
        if (mounted) setState(() => saveError = l.mainEditorContinueFailed);
      } finally {
        if (mounted) setState(() => saving = false);
      }
      return;
    }
    if (_frozenDraft == null && title.text.trim().isEmpty) {
      setState(() => invalid = true);
      return;
    }
    _submittedGeneration ??= _editGeneration;
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
      _baseline?.icon ?? Icons.auto_awesome_outlined,
      _baseline?.color ?? const Color(0xFF9D87D4),
      attachments: List.of(attachments),
      stage: stage,
      hypothesis: hypothesis.text.trim(),
      conclusion: conclusion.text.trim(),
      id: _targetId,
      versioned: _baseline?.versioned,
      contentOwner: _baseline?.contentOwner,
      contentRevision: _baseline?.contentRevision,
      favorite: _baseline?.favorite ?? widget.initialFavorite,
      time: _baseline?.time ?? '刚刚',
      todos: todos.text
          .split('\n')
          .map((line) => line.trim())
          .where((line) => line.isNotEmpty)
          .toSet()
          .toList(),
      completed: (_baseline?.completed ?? <String>{}).intersection(
        todos.text.split('\n').map((line) => line.trim()).toSet(),
      ),
    );
    setState(() {
      saving = true;
      _submitFrozen = true;
      saveError = '';
    });
    try {
      final result = _editor == null
          ? _frozenDraft!
          : await _editor!.save(_frozenDraft!, _frozenFields!);
      if (!mounted) return;
      if (!(widget.isCurrent?.call() ?? true)) {
        throw WorkbenchCommittedRefreshFailure(
          StateError('Editor workspace changed'),
        );
      }
      if (_editGeneration != _submittedGeneration) {
        _confirmedAwaitingContinuation = result;
        await _continueDraft();
      } else {
        saved = true;
        Navigator.pop(context, result);
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          if (_confirmedAwaitingContinuation != null) {
            saveError = l.mainEditorContinueFailed;
          } else if (error is EditorPreparationException) {
            _submitFrozen = false;
            _frozenDraft = null;
            _frozenFields = null;
            _submittedGeneration = null;
            saveError = error.cause is FormatException
                ? (error.cause as FormatException).message
                : l.mainSaveNotSubmitted;
          } else if (error is WorkbenchCommittedRefreshFailure) {
            saveError = l.mainContentCommittedRefreshFailed;
          } else {
            saveError = l.mainSaveUnknown;
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
        throw FormatException(l.mainAttachmentLimit);
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
      setState(() {
        attachments.add(item);
        _editGeneration++;
      });
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
                  : l.mainClipboardReadFailed,
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
      readOnly: importing,
      focusNode: descriptionFocus,
      minLines: 4,
      maxLines: 8,
      style: TextStyle(fontSize: 13, height: 1.7, color: p.ink),
      maxLength: 20000,
      decoration: InputDecoration(
        hintText: l.mainBodyHint,
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
                ? Text(l.mainPreviewEmpty, style: TextStyle(color: p.muted))
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
                Expanded(
                  child: Text(
                    l.mainMarkdownBody,
                    style: TextStyle(fontWeight: FontWeight.w600),
                  ),
                ),
                IconButton(
                  key: const ValueKey('idea-preview-toggle'),
                  tooltip: preview ? l.mainHidePreview : l.mainLivePreview,
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
        : await readPaste(reader, _plugin, l);
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
        throw FormatException(l.mainFieldLimit(limit));
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
          ? (content.files.isEmpty
                ? l.mainContentRead
                : l.mainContentReadFiles(content.files.length))
          : content.warnings.join('\n');
    });
    if (inserted.isEmpty && content.files.isEmpty && mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(l.mainClipboardEmpty)));
    }
  }

  @override
  void initState() {
    super.initState();
    _editor = widget.editor;
    _baseline = widget.initialIdea;
    _targetId =
        _editor?.targetId ?? _baseline?.id ?? widget.targetId ?? Idea.nextId();
    if (kIsWeb) ClipboardEvents.instance?.registerPasteEventListener(webPaste);
    attachments.addAll(_baseline?.attachments ?? []);
    stage =
        _baseline?.stage ??
        (widget.initialCategory == '进行中'
            ? '推进中'
            : widget.initialCategory == '实验'
            ? '待验证'
            : '待整理');
    hypothesis.text = _baseline?.hypothesis ?? '';
    conclusion.text = _baseline?.conclusion ?? '';
    final idea = _baseline;
    title.text = idea?.title ?? '';
    description.text = idea?.description ?? '';
    category = idea?.category ?? widget.initialCategory;
    todos.text = idea?.todos.join('\n') ?? '';
    for (final controller in [
      title,
      description,
      hypothesis,
      conclusion,
      todos,
    ]) {
      _lastFieldValues[controller] = controller.value;
      controller.addListener(() => _observeDraft(controller));
    }
  }

  @override
  void dispose() {
    unawaited(_editor?.close().catchError((_) {}));
    if (!identical(_preparedSuccessor, _editor)) {
      unawaited(_preparedSuccessor?.close().catchError((_) {}));
    }
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
        title: _baseline == null ? l.mainNewIdeaTitle : l.mainEditIdeaTitle,
        subtitle: l.mainEditorSubtitle,
        content: AbsorbPointer(
          absorbing: importing,
          child: SizedBox(
            width: 740,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextField(
                    key: const ValueKey('idea-title'),
                    controller: title,
                    readOnly: importing,
                    focusNode: titleFocus,
                    autofocus: true,
                    maxLength: 60,
                    style: const TextStyle(
                      fontSize: 23,
                      fontWeight: FontWeight.w600,
                    ),
                    decoration: InputDecoration(
                      hintText: l.mainIdeaNameHint,
                      errorText: invalid ? l.mainIdeaNameRequired : null,
                    ),
                  ),
                  const SizedBox(height: 12),
                  Wrap(
                    spacing: 12,
                    runSpacing: 12,
                    children: [
                      FilledButton.tonalIcon(
                        key: const ValueKey('idea-paste'),
                        onPressed: importing || saving || _submitFrozen
                            ? null
                            : () => paste(),
                        icon: const Icon(Icons.content_paste, size: 16),
                        label: Text(l.mainPasteContent),
                      ),
                      OutlinedButton.icon(
                        key: const ValueKey('idea-add-files'),
                        onPressed: importing || saving || _submitFrozen
                            ? null
                            : () => safely(
                                () async => importFiles(await openFiles()),
                              ),
                        icon: const Icon(Icons.attach_file, size: 16),
                        label: Text(l.mainImportFile),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  Text(l.mainClipboardSupport, style: TextStyle(fontSize: 11)),
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
                  if (_baseline?.versioned == null)
                    DropdownButtonFormField<String>(
                      isExpanded: true,
                      initialValue: category,
                      decoration: InputDecoration(
                        labelText: l.mainCategoryPrompt,
                      ),
                      items: ['灵感', '进行中', '实验']
                          .map(
                            (s) => DropdownMenuItem(
                              value: s,
                              child: Text(
                                s == '灵感'
                                    ? l.mainPageInbox
                                    : s == '进行中'
                                    ? l.mainPageProjects
                                    : l.mainPageLaboratory,
                              ),
                            ),
                          )
                          .toList(),
                      onChanged: (v) => setState(() {
                        _editGeneration++;
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
                          : () => setState(() {
                              attachments.remove(item);
                              _editGeneration++;
                            }),
                    ),
                  ),
                  if (category == '实验') ...[
                    const SizedBox(height: 16),
                    TextField(
                      key: const ValueKey('experiment-hypothesis'),
                      controller: hypothesis,
                      readOnly: importing,
                      focusNode: hypothesisFocus,
                      minLines: 2,
                      maxLines: 4,
                      maxLength: 5000,
                      decoration: InputDecoration(
                        labelText: l.mainHypothesisPrompt,
                      ),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      key: const ValueKey('experiment-conclusion'),
                      controller: conclusion,
                      readOnly: importing,
                      focusNode: conclusionFocus,
                      minLines: 2,
                      maxLines: 5,
                      maxLength: 10000,
                      decoration: InputDecoration(
                        labelText: l.mainObservationsPrompt,
                      ),
                    ),
                  ],
                  const SizedBox(height: 18),
                  if (_baseline?.versioned == null)
                    TextField(
                      key: const ValueKey('idea-todos'),
                      controller: todos,
                      readOnly: importing,
                      focusNode: todosFocus,
                      minLines: 2,
                      maxLines: 4,
                      maxLength: 1000,
                      decoration: InputDecoration(
                        labelText: l.mainTodosPrompt,
                        alignLabelWithHint: true,
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
        actions: [
          if (_submitFrozen || _hasUnsavedSuccessor || saveError.isNotEmpty)
            SizedBox(
              width: double.infinity,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 96),
                child: SingleChildScrollView(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (_submitFrozen || _hasUnsavedSuccessor)
                        Text(
                          _submitFrozen
                              ? l.mainEditorPendingDraft
                              : l.mainEditorNewerDraft,
                          key: const ValueKey('idea-draft-status'),
                        ),
                      if (saveError.isNotEmpty)
                        Text(
                          saveError,
                          key: const ValueKey('idea-save-error'),
                          style: TextStyle(
                            color: Theme.of(context).colorScheme.error,
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          TextButton(
            onPressed: importing || saving
                ? null
                : () => Navigator.pop(context),
            child: Text(l.mainNotNow),
          ),
          FilledButton(
            key: const ValueKey('idea-save'),
            // Keep focus while a save is pending. Disabling the focused Web
            // button returns focus to the autofocus title field and changes
            // its selection, falsely creating a successor draft. _save guards
            // repeated activation while saving; actual edits remain tracked.
            onPressed: importing ? null : _save,
            child: Text(
              saving
                  ? l.mainSaving
                  : _confirmedAwaitingContinuation != null
                  ? l.mainEditorContinueDraft
                  : _submitFrozen
                  ? l.mainRetrySave
                  : l.mainSaveIdea,
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
