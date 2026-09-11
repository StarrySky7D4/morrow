import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
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
  int _epoch = 0, _updates = 0, _editTicket = 0, _pending = 0;
  bool _commandPending = false;
  Color _accent = const Color(0xff69dfc5);
  Future<void> _queue = Future.value();
  final _log = <String>[];
  static const labels = {'rust': 'Rust', 'c': 'C', 'cpp': 'C++'};
  static const names = {'rust': '文字工坊', 'c': '单位换算', 'cpp': '清单助手'};
  static const descriptions = {
    'rust': '整理段落、合并空白，实时查看文字统计。',
    'c': '温度与长度双向换算，直接修改数值即可得到结果。',
    'cpp': '把多行文字变成清单，勾选、排序，再清除已完成项。',
  };
  static const icons = {
    'rust': Icons.notes_rounded,
    'c': Icons.swap_horiz_rounded,
    'cpp': Icons.checklist_rounded,
  };
  String _value(String id) =>
      _document?.nodes.where((n) => n.id == id).firstOrNull?.text ?? '';
  UiDocumentModel get _controls => UiDocumentModel(
    _document!.nodes
        .where((n) => !{'result', 'detail', 'caption'}.contains(n.id))
        .map(
          (n) => UiNode(
            id: n.id,
            parent: n.parent,
            kind: n.kind,
            label: n.label,
            text: n.text,
            action: n.action,
            checked: n.checked,
            maxBytes: n.maxBytes,
            tone: n.tone,
            enabled:
                n.enabled &&
                (n.kind == Kind.textInput
                    ? !_commandPending
                    : [Kind.button, Kind.toggle].contains(n.kind)
                    ? _pending == 0
                    : true),
          ),
        )
        .toList(),
  );

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
      _accent = switch (language) {
        'c' => const Color(0xffe1b774),
        'cpp' => const Color(0xffb4a3f3),
        _ => const Color(0xff69dfc5),
      };
      _loading = true;
      _failure = null;
      _document = null;
      _updates = 0;
      _pending = 0;
      _commandPending = false;
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
    // Text edits can queue, but semantic actions wait for the acknowledged view.
    // In particular a checkbox from before a sort cannot target a different item.
    if (_commandPending ||
        (intent.kind != EventKind.editText && _pending > 0)) {
      return;
    }
    setState(() {
      _pending++;
      _commandPending = intent.kind != EventKind.editText;
    });
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
      } finally {
        if (mounted && epoch == _epoch) {
          setState(() {
            _pending--;
            _commandPending = false;
          });
        }
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
      Future<void> send(
        String node,
        String action,
        EventKind kind, {
        String text = '',
        bool checked = false,
      }) async {
        _intent(
          UiIntent(
            node: node,
            action: action,
            kind: kind,
            text: text,
            checked: checked,
          ),
        );
        await _queue;
        if (_failure != null) throw StateError('$_language: $_failure');
      }

      void expectValue(String id, String expected) {
        if (_value(id) != expected) {
          throw StateError(
            '$_language $id: expected "$expected", got "${_value(id)}"',
          );
        }
      }

      await _select('rust');
      await send(
        'title',
        'edit',
        EventKind.editText,
        text: '  alpha   beta  \n gamma \n',
      );
      expectValue('result', 'alpha   beta\ngamma');
      await send('option', 'compact', EventKind.setToggle, checked: true);
      expectValue('result', 'alpha beta gamma');
      await send('apply', 'tidy', EventKind.activate);
      expectValue('title', 'alpha beta gamma');
      await send('reset', 'clear', EventKind.activate);
      expectValue('result', '');
      await send(
        'title',
        'edit',
        EventKind.editText,
        text: '  Morrow   灵感工坊  \n  写下每一个想法  ',
      );
      await send('option', 'compact', EventKind.setToggle, checked: false);
      await _screenshot('rust-dark');
      await _select('c');
      await send('title', 'edit', EventKind.editText, text: '100');
      expectValue('result', '212.000 °F');
      await send('reverse', 'reverse', EventKind.setToggle, checked: true);
      expectValue('result', '37.778 °C');
      await send('mode', 'mode', EventKind.setToggle, checked: true);
      expectValue('result', '30.480 米');
      await send('reverse', 'reverse', EventKind.setToggle, checked: false);
      await send('title', 'edit', EventKind.editText, text: 'abc');
      expectValue('result', '等待有效数值');
      await send('example', 'example', EventKind.activate);
      expectValue('result', '3.281 英尺');
      await _screenshot('c-dark');
      await _select('cpp');
      await send(
        'title',
        'edit',
        EventKind.editText,
        text: 'Zebra\nAlpha\nBeta',
      );
      await send('item0', 'item0', EventKind.setToggle, checked: true);
      await send('sort', 'sort', EventKind.activate);
      expectValue('result', '○ Alpha\n○ Beta\n✓ Zebra');
      await send('option', 'hide', EventKind.setToggle, checked: true);
      expectValue('result', '○ Alpha\n○ Beta');
      await send('clear', 'clear', EventKind.activate);
      expectValue('title', 'Alpha\nBeta');
      expectValue('detail', '已完成 0 / 2 · 0%');
      await send('reset', 'reset', EventKind.activate);
      await send('item0', 'item0', EventKind.setToggle, checked: true);
      await _screenshot('cpp-dark');
      setState(() {
        _dark = false;
        _accent = const Color(0xffab6c46);
      });
      await _screenshot('light');
      setState(() => _narrow = true);
      await _screenshot('narrow');
      await _bridge?.close();
      await File('${widget.captureFolder}/runtime-check.txt').writeAsString(
        'PASS: actual Windows release; 3 distinct live plugins; text tidy/clear, bidirectional units and invalid input recovery, checklist toggle/sort/filter/clear; theme and narrow captures.\n${_log.join('\n')}\n',
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
      title: 'Morrow · 差异化展示 Demo 02',
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
                                            '差异化展示 02  /  Windows',
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
                                const SizedBox(height: 22),
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
                                  '三个插件，三种用法。让文字、数值与待办，都有自己的工作方式。',
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
                                        label: Text(
                                          '${names[item.key]} · ${item.value}',
                                        ),
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
                                  '0.1.9-test.10 / Demo 02  ·  独立会话  ·  不保存到正式资料库',
                                  style: TextStyle(
                                    fontSize: 11,
                                    color: s.onSurfaceVariant,
                                  ),
                                ),
                                const SizedBox(height: 8),
                                Text(
                                  '切换工具会开启新会话。文字整理、单位换算与清单管理均由真实插件处理。',
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
                  names[_language]!,
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
            descriptions[_language]!,
            style: TextStyle(fontSize: 11, color: s.onSurfaceVariant),
          ),
          const SizedBox(height: 23),
          ConstrainedBox(
            constraints: BoxConstraints(
              minHeight: 245,
              maxHeight: _language == 'cpp' ? 560 : 420,
            ),
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
                : TweenAnimationBuilder<double>(
                    key: ValueKey(_epoch),
                    tween: Tween(begin: 0, end: 1),
                    duration: const Duration(milliseconds: 300),
                    builder: (context, t, child) => Opacity(
                      opacity: t,
                      child: Transform.translate(
                        offset: Offset(0, 10 * (1 - t)),
                        child: child,
                      ),
                    ),
                    child: PluginForm(
                      document: _controls,
                      viewIdentity: _epoch,
                      onIntent: _intent,
                    ),
                  ),
          ),
        ],
      ),
    );
  }

  Widget _preview(BuildContext context) {
    final s = Theme.of(context).colorScheme;
    final result = _value('result');
    final title = switch (_language) {
      'c' => '换算结果',
      'cpp' => '我的清单',
      _ => '整理后的文字',
    };
    return _panel(
      context,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  title,
                  style: TextStyle(fontSize: 13, color: s.onSurfaceVariant),
                ),
              ),
              IconButton(
                tooltip: '重新开始当前工具',
                onPressed: _loading ? null : () => _select(_language),
                icon: const Icon(Icons.restart_alt_rounded),
              ),
              IconButton(
                tooltip: '复制结果',
                onPressed: result.isEmpty || _loading || _failure != null
                    ? null
                    : () async {
                        await Clipboard.setData(ClipboardData(text: result));
                        if (context.mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(content: Text('结果已复制')),
                          );
                        }
                      },
                icon: const Icon(Icons.copy_rounded, size: 19),
              ),
            ],
          ),
          const SizedBox(height: 20),
          Container(
            padding: const EdgeInsets.all(15),
            decoration: BoxDecoration(
              color: s.primaryContainer,
              borderRadius: BorderRadius.circular(20),
            ),
            child: Icon(
              icons[_language],
              color: s.onPrimaryContainer,
              size: 28,
            ),
          ),
          const SizedBox(height: 22),
          if (_language == 'cpp' && result.isNotEmpty)
            ...result
                .split('\n')
                .map(
                  (line) => Container(
                    width: double.infinity,
                    margin: const EdgeInsets.only(bottom: 8),
                    padding: const EdgeInsets.all(12),
                    decoration: BoxDecoration(
                      color: s.primaryContainer.withValues(alpha: .25),
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Text(
                      line,
                      style: TextStyle(
                        fontSize: 16,
                        height: 1.6,
                        color: s.onSurface,
                      ),
                    ),
                  ),
                )
          else
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 230),
              child: SingleChildScrollView(
                child: SelectableText(
                  _loading
                      ? '正在连接…'
                      : result.isEmpty
                      ? '等待新的文字'
                      : result,
                  style: TextStyle(
                    fontSize: _language == 'c' ? 34 : 21,
                    fontWeight: _language == 'c'
                        ? FontWeight.w700
                        : FontWeight.w500,
                    height: 1.65,
                    color: s.onSurface,
                  ),
                ),
              ),
            ),
          const SizedBox(height: 20),
          Text(
            _value('detail'),
            key: const ValueKey('plugin-detail'),
            style: TextStyle(fontSize: 12, color: s.primary, height: 1.7),
          ),
          const SizedBox(height: 16),
          Text(
            _value('caption'),
            style: TextStyle(
              fontSize: 11,
              color: s.onSurfaceVariant,
              height: 1.7,
            ),
          ),
          const SizedBox(height: 20),
          Row(
            children: [
              Icon(
                Icons.circle,
                size: 6,
                color: _failure == null ? s.primary : s.error,
              ),
              const SizedBox(width: 7),
              Text(
                _failure != null
                    ? '连接中断，结果为上次返回'
                    : _updates == 0
                    ? '试试左侧的工具'
                    : '已收到插件回应',
                style: TextStyle(fontSize: 11, color: s.onSurfaceVariant),
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
