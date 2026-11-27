import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

@immutable
class DetachedWindowTheme {
  const DetachedWindowTheme({
    required this.dark,
    required this.surfaceArgb,
    required this.bgArgb,
    required this.inkArgb,
    required this.mutedArgb,
    required this.accentArgb,
    required this.onAccentArgb,
    required this.lineArgb,
    required this.edgeArgb,
    this.cornerRadius = 20.0,
    this.frostedOpacity = 0.88,
    this.isSolid = false,
    this.fontFamily = 'Microsoft YaHei UI',
  });

  final bool dark;
  final int surfaceArgb;
  final int bgArgb;
  final int inkArgb;
  final int mutedArgb;
  final int accentArgb;
  final int onAccentArgb;
  final int lineArgb;
  final int edgeArgb;
  final double cornerRadius;
  final double frostedOpacity;
  final bool isSolid;
  final String fontFamily;

  Map<String, Object> toMap() => {
    'dark': dark,
    'surfaceArgb': surfaceArgb,
    'bgArgb': bgArgb,
    'inkArgb': inkArgb,
    'mutedArgb': mutedArgb,
    'accentArgb': accentArgb,
    'onAccentArgb': onAccentArgb,
    'lineArgb': lineArgb,
    'edgeArgb': edgeArgb,
    'cornerRadius': cornerRadius,
    'frostedOpacity': frostedOpacity,
    'isSolid': isSolid,
    'fontFamily': fontFamily,
  };

  String get signature =>
      '$dark|$surfaceArgb|$bgArgb|$inkArgb|$accentArgb|$edgeArgb|${cornerRadius.toStringAsFixed(1)}|${frostedOpacity.toStringAsFixed(2)}|$isSolid|$fontFamily';
}

@immutable
class DetachedEditorPayload {
  const DetachedEditorPayload({
    required this.id,
    required this.title,
    required this.description,
    required this.category,
    required this.todos,
  });

  final String id;
  final String title;
  final String description;
  final String category;
  final String todos;
}

/// Native Windows always-on-top mini window bridge (`morrow/detached_widgets`).
class DetachedWindowBridge {
  DetachedWindowBridge({MethodChannel? channel})
    : _channel = channel ?? defaultChannel {
    _channel.setMethodCallHandler(_handleMethodCall);
  }

  static const MethodChannel defaultChannel = MethodChannel(
    'morrow/detached_widgets',
  );

  final MethodChannel _channel;
  void Function(String id, String action)? onNativeAction;
  void Function(String id, String action, DetachedEditorPayload payload)?
  onEditorAction;

  bool get supportedPlatform =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.windows;

  Future<dynamic> _handleMethodCall(MethodCall call) async {
    if (call.method == 'onWidgetAction' && call.arguments is Map) {
      final args = call.arguments as Map;
      final id = args['id'];
      final action = args['action'];
      if (id is String && action is String) {
        onNativeAction?.call(id, action);
      }
    } else if (call.method == 'onEditorAction' && call.arguments is Map) {
      final args = call.arguments as Map;
      final id = args['id'];
      final action = args['action'];
      if (id is String && action is String) {
        onEditorAction?.call(
          id,
          action,
          DetachedEditorPayload(
            id: id,
            title: args['title'] as String? ?? '',
            description: args['description'] as String? ?? '',
            category: args['category'] as String? ?? '灵感',
            todos: args['todos'] as String? ?? '',
          ),
        );
      }
    }
    return null;
  }

  Future<bool> isSupported() async {
    if (!supportedPlatform) return false;
    try {
      return (await _channel.invokeMethod<bool>('isSupported')) ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  Future<void> openOrUpdate({
    required String id,
    required String kind,
    required String title,
    required String primaryText,
    required String secondaryText,
    required String badgeText,
    required double progress,
    required bool running,
    required bool pinned,
    required bool dark,
    required int accentArgb,
    required int bgArgb,
    DetachedWindowTheme? theme,
    int width = 320,
    int height = 205,
    int? relX,
    int? relY,
  }) async {
    if (!supportedPlatform) return;
    try {
      await _channel.invokeMethod<void>('openOrUpdate', {
        'id': id,
        'kind': kind,
        'title': title,
        'primaryText': primaryText,
        'secondaryText': secondaryText,
        'badgeText': badgeText,
        'progress': progress.clamp(0.0, 1.0),
        'running': running,
        'pinned': pinned,
        'dark': dark,
        'accentArgb': accentArgb,
        'bgArgb': bgArgb,
        if (theme != null) ...theme.toMap(),
        'width': width,
        'height': height,
        'relX': ?relX,
        'relY': ?relY,
      });
    } on PlatformException {
      // Gracefully ignore if test runner or OS blocks window creation.
    } on MissingPluginException {
      // Headless / test environment without native channel.
    }
  }

  Future<void> openEditor({
    required String id,
    required String windowTitle,
    required String title,
    required String description,
    required String category,
    required String todos,
    required bool pinned,
    required bool dark,
    required int accentArgb,
    required int bgArgb,
    DetachedWindowTheme? theme,
    int width = 620,
    int height = 520,
    int? relX,
    int? relY,
    bool previewMode = false,
    bool updateThemeOnly = false,
  }) async {
    if (!supportedPlatform) return;
    try {
      await _channel.invokeMethod<void>('openEditor', {
        'id': id,
        'windowTitle': windowTitle,
        'title': title,
        'description': description,
        'category': category,
        'todos': todos,
        'pinned': pinned,
        'dark': dark,
        'accentArgb': accentArgb,
        'bgArgb': bgArgb,
        if (theme != null) ...theme.toMap(),
        'width': width,
        'height': height,
        'relX': ?relX,
        'relY': ?relY,
        'previewMode': previewMode,
        if (updateThemeOnly) 'updateThemeOnly': true,
      });
    } on PlatformException {
      // Ignore if test runner / OS blocks window creation.
    } on MissingPluginException {
      // Ignore in headless test runs.
    }
  }

  Future<void> close(String id) async {
    if (!supportedPlatform) return;
    try {
      await _channel.invokeMethod<void>('close', {'id': id});
    } on PlatformException {
      /* no-op */
    } on MissingPluginException {
      /* no-op */
    }
  }

  Future<void> closeAll() async {
    if (!supportedPlatform) return;
    try {
      await _channel.invokeMethod<void>('closeAll');
    } on PlatformException {
      /* no-op */
    } on MissingPluginException {
      /* no-op */
    }
  }

  void dispose() {
    _channel.setMethodCallHandler(null);
  }
}
