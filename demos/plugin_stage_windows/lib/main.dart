import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:morrow_core_client/ui.dart' show UiDocument;
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';
import 'demo_bridge.dart';

void main(List<String> args) {
  WidgetsFlutterBinding.ensureInitialized();
  final capture = args.where((a) => a.startsWith('--showcase=')).firstOrNull;
  runApp(StageDemo(captureFolder: capture?.substring('--showcase='.length)));
}

class StageDemo extends StatefulWidget {
  const StageDemo({super.key, this.captureFolder, this.bundleFolder});
  final String? captureFolder;
  final String? bundleFolder;
  @override
  State<StageDemo> createState() => _StageDemoState();
}

class _StageDemoState extends State<StageDemo> {
  final _capture = GlobalKey();
  DemoBridge? _bridge;
  UiDocument? _document;
  String _language = 'rust';
  String? _failure;
  bool _loading = true, _dark = true, _narrow = false;
  int _epoch = 0, _updates = 0, _editTicket = 0;
  Color _accent = const Color(0xff69dfc5);
  Future<void> _queue = Future.value();
  final _log = <String>[];
  static const labels = {'rust': 'Rust', 'c': 'C', 'cpp': 'C++'};
  String get _folder =>
      widget.bundleFolder ?? File(Platform.resolvedExecutable).parent.path;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await _select('rust');
      if (widget.captureFolder != null) unawaited(_showcase());
    });
  }

  Future<void> _select(String language) async {
    final epoch = ++_epoch;
    final old = _bridge;
    _bridge = null;
    setState(() {
      _language = language;
      _loading = true;
      _failure = null;
      _document = null;
      _updates = 0;
    });
    _queue = Future.value();
    await old?.close();
    try {
      final bridge = await DemoBridge.open(_folder, language);
      if (!mounted || epoch != _epoch) {
        await bridge.close();
        return;
      }
      _bridge = bridge;
      setState(() {
        _document = bridge.document;
        _loading = false;
      });
      _log.add('$language: connected revision ${bridge.revision}');
    } catch (e) {
      if (mounted && epoch == _epoch) {
        setState(() {
          _failure = '插件连接失败，请重新连接。';
          _loading = false;
        });
      }
      _log.add('$language: $e');
      assert(() {
        debugPrint('Demo open error: $e');
        return true;
      }());
    }
  }

  void _intent(UiIntent intent) {
    final epoch = _epoch;
    final ticket = ++_editTicket;
    _queue = _queue.then((_) async {
      if (!mounted || epoch != _epoch || _bridge == null) return;
      try {
        final document = await _bridge!.edit(intent);
        if (mounted && epoch == _epoch) {
          setState(() {
            if (ticket == _editTicket) _document = document;
            _updates++;
          });
        }
        _log.add('$_language: revision ${_bridge?.revision}; ${intent.text}');
      } catch (e) {
        if (mounted && epoch == _epoch) {
          setState(() => _failure = '连接已中断，可重新连接开始新会话。');
        }
        _log.add('$_language: $e');
      }
    });
  }

  Future<void> _screenshot(String name) async {
    await Future<void>.delayed(const Duration(milliseconds: 450));
    await WidgetsBinding.instance.endOfFrame;
    final render =
        _capture.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await render.toImage(pixelRatio: 1.5);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    image.dispose();
    final file = File('${widget.captureFolder}/$name.png');
    await file.parent.create(recursive: true);
    await file.writeAsBytes(bytes!.buffer.asUint8List());
  }

  Future<void> _showcase() async {
    try {
      for (final lang in ['rust', 'c', 'cpp']) {
        await _select(lang);
        if (_bridge == null) throw StateError('Plugin unavailable: $lang');
        final title = switch (lang) {
          'rust' => '灵感，从这里开始',
          'c' => '把想法变成卡片',
          _ => '同一界面，三种能力',
        };
        _intent(
          UiIntent(
            node: 'title',
            action: 'title.edit',
            kind: EventKind.editText,
            text: title,
          ),
        );
        await _queue;
        if (_failure != null ||
            _document!.nodes.firstWhere((n) => n.id == 'title').text != title ||
            _bridge!.revision != BigInt.from(2)) {
          throw StateError('Live round trip failed: $lang');
        }
        await _screenshot('$lang-dark');
      }
      setState(() {
        _dark = false;
        _accent = const Color(0xffab6c46);
      });
      await _screenshot('light');
      setState(() => _narrow = true);
      await _screenshot('narrow');
      await _bridge?.close();
      await File('${widget.captureFolder}/runtime-check.txt').writeAsString(
        'PASS: actual Windows release; 3 live plugin sessions and title round trips; theme and narrow presentation captures.\n${_log.join('\n')}\n',
      );
      exit(0);
    } catch (e) {
      await Directory(widget.captureFolder!).create(recursive: true);
      await File(
        '${widget.captureFolder}/runtime-check.txt',
      ).writeAsString('FAIL: $e\n${_log.join('\n')}');
      await _bridge?.close();
      exit(1);
    }
  }

  @override
  void dispose() {
    _epoch++;
    unawaited(_bridge?.close() ?? Future.value());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = ColorScheme.fromSeed(
      seedColor: _accent,
      brightness: _dark ? Brightness.dark : Brightness.light,
    );
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Morrow · 阶段展示',
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: scheme,
        scaffoldBackgroundColor: Colors.transparent,
        fontFamily: 'Microsoft YaHei',
        inputDecorationTheme: InputDecorationTheme(
          filled: true,
          fillColor: scheme.surfaceContainerHighest.withValues(alpha: .6),
          contentPadding: const EdgeInsets.symmetric(
            horizontal: 18,
            vertical: 18,
          ),
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(18),
            borderSide: BorderSide(color: scheme.outlineVariant),
          ),
        ),
        filledButtonTheme: FilledButtonThemeData(
          style: FilledButton.styleFrom(
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(14),
            ),
          ),
        ),
      ),
      home: Builder(
        builder: (context) {
          final s = Theme.of(context).colorScheme;
          return RepaintBoundary(
            key: _capture,
            child: Scaffold(
              body: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topLeft,
                    end: Alignment.bottomRight,
                    colors: _dark
                        ? [
                            const Color(0xff101c2c),
                            const Color(0xff142c32),
                            const Color(0xff161b2b),
                          ]
                        : [
                            const Color(0xfff7f3eb),
                            const Color(0xffe8f0eb),
                            const Color(0xffeef0f7),
                          ],
                  ),
                ),
                child: SafeArea(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      final width = _narrow ? 480.0 : constraints.maxWidth;
                      return Center(
                        child: SizedBox(
                          width: width,
                          child: SingleChildScrollView(
                            padding: EdgeInsets.symmetric(
                              horizontal: width < 650 ? 22 : 44,
                              vertical: 28,
                            ),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Row(
                                  children: [
                                    Container(
                                      padding: const EdgeInsets.all(11),
                                      decoration: BoxDecoration(
                                        color: s.primaryContainer,
                                        borderRadius: BorderRadius.circular(15),
                                      ),
                                      child: Icon(
                                        Icons.all_inclusive_rounded,
                                        color: s.onPrimaryContainer,
                                        size: 25,
                                      ),
                                    ),
                                    const SizedBox(width: 13),
                                    Expanded(
                                      child: Column(
                                        crossAxisAlignment:
                                            CrossAxisAlignment.start,
                                        children: [
                                          Text(
                                            'Morrow',
                                            style: TextStyle(
                                              fontSize: 23,
                                              fontWeight: FontWeight.w700,
                                              color: s.onSurface,
                                            ),
                                          ),
                                          Text(
                                            '阶段展示  /  Windows',
                                            style: TextStyle(
                                              fontSize: 12,
                                              color: s.onSurfaceVariant,
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                    IconButton(
                                      tooltip: '切换明暗主题',
                                      onPressed: () =>
                                          setState(() => _dark = !_dark),
                                      icon: Icon(
                                        _dark
                                            ? Icons.light_mode_outlined
                                            : Icons.dark_mode_outlined,
                                      ),
                                    ),
                                    IconButton(
                                      tooltip: '切换窄屏预览',
                                      onPressed: () =>
                                          setState(() => _narrow = !_narrow),
                                      icon: Icon(
                                        _narrow
                                            ? Icons.desktop_windows_outlined
                                            : Icons.view_sidebar_outlined,
                                      ),
                                    ),
                                  ],
                                ),
                                const SizedBox(height: 34),
                                Text(
                                  '让能力，长在卡片上。',
                                  style: TextStyle(
                                    fontSize: width < 650 ? 28 : 38,
                                    fontWeight: FontWeight.w700,
                                    height: 1.2,
                                    letterSpacing: -1,
                                    color: s.onSurface,
                                  ),
                                ),
                                const SizedBox(height: 12),
                                Text(
                                  '三种语言，共用一套界面。编辑标题，看看插件如何即时回应。',
                                  style: TextStyle(
                                    fontSize: 14,
                                    height: 1.8,
                                    color: s.onSurfaceVariant,
                                  ),
                                ),
                                const SizedBox(height: 25),
                                Wrap(
                                  spacing: 10,
                                  runSpacing: 10,
                                  children: [
                                    for (final item in labels.entries)
                                      ChoiceChip(
                                        label: Text('${item.value} 插件'),
                                        selected: _language == item.key,
                                        onSelected: (_) => _select(item.key),
                                        showCheckmark: false,
                                        padding: const EdgeInsets.symmetric(
                                          horizontal: 14,
                                          vertical: 10,
                                        ),
                                      ),
                                    Container(
                                      padding: const EdgeInsets.symmetric(
                                        horizontal: 14,
                                        vertical: 12,
                                      ),
                                      child: Row(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                          Container(
                                            width: 7,
                                            height: 7,
                                            decoration: BoxDecoration(
                                              shape: BoxShape.circle,
                                              color: _failure != null
                                                  ? s.error
                                                  : _loading
                                                  ? s.outline
                                                  : s.primary,
                                            ),
                                          ),
                                          const SizedBox(width: 8),
                                          Text(
                                            _failure != null
                                                ? '连接中断'
                                                : _loading
                                                ? '正在连接'
                                                : '插件已连接',
                                            style: TextStyle(
                                              color: s.onSurfaceVariant,
                                              fontSize: 12,
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                  ],
                                ),
                                const SizedBox(height: 24),
                                if (width < 850) ...[
                                  _editor(context),
                                  const SizedBox(height: 18),
                                  _preview(context),
                                ] else
                                  Row(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Expanded(
                                        flex: 6,
                                        child: _editor(context),
                                      ),
                                      const SizedBox(width: 22),
                                      Expanded(
                                        flex: 5,
                                        child: _preview(context),
                                      ),
                                    ],
                                  ),
                                const SizedBox(height: 22),
                                _panel(
                                  context,
                                  child: Wrap(
                                    spacing: 25,
                                    runSpacing: 18,
                                    alignment: WrapAlignment.spaceBetween,
                                    children: [
                                      _step(
                                        context,
                                        Icons.extension_outlined,
                                        '真实插件',
                                        'C / C++ / Rust',
                                      ),
                                      _step(
                                        context,
                                        Icons.verified_outlined,
                                        '宿主校验',
                                        '事件检查后交付',
                                      ),
                                      _step(
                                        context,
                                        Icons.auto_awesome_outlined,
                                        '界面更新',
                                        '本次已更新 $_updates 次',
                                      ),
                                      Row(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                          Text(
                                            '主题色',
                                            style: TextStyle(
                                              fontSize: 12,
                                              color: s.onSurfaceVariant,
                                            ),
                                          ),
                                          const SizedBox(width: 12),
                                          for (final color in [
                                            const Color(0xff69dfc5),
                                            const Color(0xffa69bed),
                                            const Color(0xffd9a079),
                                          ])
                                            Padding(
                                              padding: const EdgeInsets.only(
                                                right: 8,
                                              ),
                                              child: Semantics(
                                                label: '切换主题色',
                                                button: true,
                                                child: InkWell(
                                                  onTap: () => setState(
                                                    () => _accent = color,
                                                  ),
                                                  borderRadius:
                                                      BorderRadius.circular(20),
                                                  child: Container(
                                                    width: 25,
                                                    height: 25,
                                                    decoration: BoxDecoration(
                                                      shape: BoxShape.circle,
                                                      color: color,
                                                      border: Border.all(
                                                        color: _accent == color
                                                            ? s.onSurface
                                                            : Colors
                                                                  .transparent,
                                                        width: 2,
                                                      ),
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
                                const SizedBox(height: 20),
                                Text(
                                  '0.1.9-test.10  ·  独立演示数据  ·  不保存到正式资料库',
                                  style: TextStyle(
                                    fontSize: 11,
                                    color: s.onSurfaceVariant,
                                  ),
                                ),
                                const SizedBox(height: 8),
                                Text(
                                  '当前展示插件表单与标题交互；插件安装管理、持久草稿和正式保存仍在建设。',
                                  style: TextStyle(
                                    fontSize: 11,
                                    height: 1.6,
                                    color: s.onSurfaceVariant,
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
            ),
          );
        },
      ),
    );
  }

  Widget _panel(BuildContext context, {required Widget child}) {
    final s = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.all(25),
      decoration: BoxDecoration(
        color: s.surface.withValues(alpha: _dark ? 0.65 : 0.8),
        borderRadius: BorderRadius.circular(25),
        border: Border.all(color: s.outlineVariant.withValues(alpha: .55)),
        boxShadow: [
          BoxShadow(
            color: Colors.black.withValues(alpha: _dark ? 0.1 : 0.035),
            blurRadius: 24,
            offset: const Offset(0, 10),
          ),
        ],
      ),
      child: child,
    );
  }

  Widget _editor(BuildContext context) {
    final s = Theme.of(context).colorScheme;
    return _panel(
      context,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '插件工作区',
                  style: TextStyle(
                    fontSize: 17,
                    fontWeight: FontWeight.w600,
                    color: s.onSurface,
                  ),
                ),
              ),
              Text(
                '${labels[_language]}',
                style: TextStyle(color: s.primary, fontWeight: FontWeight.w600),
              ),
            ],
          ),
          const SizedBox(height: 7),
          Text(
            '标题可实时编辑，其他控件仅作样式预览。',
            style: TextStyle(fontSize: 11, color: s.onSurfaceVariant),
          ),
          const SizedBox(height: 23),
          SizedBox(
            height: 255,
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : _failure != null
                ? Center(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(_failure!, textAlign: TextAlign.center),
                        const SizedBox(height: 15),
                        FilledButton(
                          onPressed: () => _select(_language),
                          child: const Text('重新连接'),
                        ),
                      ],
                    ),
                  )
                : PluginForm(
                    document: _document!,
                    viewIdentity: _epoch,
                    onIntent: _intent,
                  ),
          ),
        ],
      ),
    );
  }

  Widget _preview(BuildContext context) {
    final s = Theme.of(context).colorScheme;
    final title =
        _document?.nodes.firstWhere((n) => n.id == 'title').text ?? '等待新的灵感';
    return _panel(
      context,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '卡片预览',
            style: TextStyle(fontSize: 12, color: s.onSurfaceVariant),
          ),
          const SizedBox(height: 27),
          Container(
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: s.primaryContainer,
              borderRadius: BorderRadius.circular(19),
            ),
            child: Icon(
              Icons.lightbulb_outline_rounded,
              color: s.onPrimaryContainer,
              size: 26,
            ),
          ),
          const SizedBox(height: 22),
          Text(
            title.isEmpty ? '未命名灵感' : title,
            style: TextStyle(
              fontSize: 25,
              fontWeight: FontWeight.w600,
              height: 1.5,
              color: s.onSurface,
            ),
          ),
          const SizedBox(height: 13),
          Text(
            '这张卡片的标题来自插件返回的界面。\n你可以切换语言，体验相同的交互。',
            style: TextStyle(
              fontSize: 12,
              height: 1.8,
              color: s.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: 28),
          Row(
            children: [
              Icon(Icons.circle, size: 6, color: s.primary),
              const SizedBox(width: 7),
              Text(
                _updates == 0 ? '等待你的第一笔' : '已收到插件回应',
                style: TextStyle(fontSize: 11, color: s.primary),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _step(
    BuildContext context,
    IconData icon,
    String title,
    String subtitle,
  ) {
    final s = Theme.of(context).colorScheme;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, color: s.primary, size: 20),
        const SizedBox(width: 11),
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              title,
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w600,
                color: s.onSurface,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              subtitle,
              style: TextStyle(fontSize: 10, color: s.onSurfaceVariant),
            ),
          ],
        ),
      ],
    );
  }
}
