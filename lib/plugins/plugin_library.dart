import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:morrow_plugin_ui/online.dart';

class PluginLibraryPage {
  const PluginLibraryPage({
    required this.revision,
    required this.entries,
    required this.cursor,
  });
  final BigInt revision;
  final List<PluginLibraryEntry> entries;
  final String cursor;
}

class PluginLibraryEntry {
  const PluginLibraryEntry({
    required this.id,
    required this.name,
    required this.version,
    required this.digest,
    required this.enabled,
    required this.builtin,
    required this.available,
    required this.declared,
    required this.approved,
    required this.dependencies,
    required this.handlers,
    required this.issue,
  });
  final String id, name, version, issue;
  final Uint8List digest;
  final bool enabled, builtin, available;
  final List<String> declared, approved, dependencies;
  final List<PluginTransformHandler> handlers;
}

class PluginTransformHandler {
  const PluginTransformHandler({
    required this.name,
    required this.inputType,
    required this.outputType,
    required this.maxInputBytes,
    required this.maxOutputBytes,
  });
  final String name, inputType, outputType;
  final int maxInputBytes, maxOutputBytes;
}

abstract interface class ExternalPluginControl {
  Future<PluginLibraryPage> pluginPage({String cursor = '', BigInt? revision});
  Future<PluginLibraryPage> inspectPlugin(String path);
  Future<void> importPlugin(String path, Uint8List digest, BigInt revision);
  Future<void> configureExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    List<String> approved,
    bool enable,
  );
  Future<void> removeExternal(PluginLibraryEntry entry, BigInt revision);
  Future<Uint8List> transformExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    PluginTransformHandler handler,
    Uint8List input,
  );
  PluginUiTransport createExternalPluginUi(
    PluginLibraryEntry entry,
    BigInt revision,
  );
}

class PluginLibrary extends StatefulWidget {
  const PluginLibrary({
    super.key,
    required this.backend,
    required this.onChanged,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.pickPackage,
  });
  final ExternalPluginControl backend;
  final VoidCallback onChanged;
  final Color ink, muted, line;
  final BorderRadius radius;
  final Future<String?> Function()? pickPackage;
  @override
  State<PluginLibrary> createState() => _PluginLibraryState();
}

// Controller.close intentionally hides transport errors. Preserve the underlying
// Future here so explicit library actions can report failure before changing policy.
class _ObservedTransport implements PluginUiTransport {
  _ObservedTransport(this.delegate);
  final PluginUiTransport delegate;
  Future<void>? _closing;
  @override
  Future<PluginUiReply> open(String seed) => delegate.open(seed);
  @override
  Future<PluginUiReply> event(Uint8List bytes) => delegate.event(bytes);
  @override
  Future<void> close() => _closing ??= Future<void>.sync(delegate.close);
  Future<void> retryClose() {
    _closing = null;
    return close();
  }
}

class _PluginLibraryState extends State<PluginLibrary> {
  List<PluginLibraryEntry> _entries = [];
  BigInt? _revision;
  bool _busy = false, _confirmed = false;
  String? _message, _candidatePath, _toolId, _result, _fileName;
  PluginLibraryPage? _preview;
  final Map<String, Set<String>> _approvals = {};
  final _text = TextEditingController();
  PluginTransformHandler? _handler;
  Uint8List? _fileBytes;
  PluginUiController? _controller;
  _ObservedTransport? _transport, _unclosed;
  String? _formId;

  bool _current(ExternalPluginControl backend) =>
      mounted && identical(widget.backend, backend);
  @override
  void initState() {
    super.initState();
    unawaited(_refresh());
  }

  @override
  void didUpdateWidget(covariant PluginLibrary oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      _releaseForm();
      _busy = false;
      _confirmed = false;
      _entries = [];
      _revision = null;
      _preview = null;
      _clearTool();
      unawaited(_refresh());
    }
  }

  void _releaseForm() {
    final controller = _controller;
    final transport = _transport ?? _unclosed;
    _controller = null;
    _transport = null;
    _unclosed = null;
    _formId = null;
    if (transport == null) return;
    unawaited(() async {
      try {
        if (controller != null) {
          await controller.close();
          await transport.close();
        } else {
          await transport.retryClose();
        }
      } catch (error, stack) {
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stack,
            library: 'plugin library',
            context: ErrorDescription('closing an external plugin view'),
          ),
        );
      } finally {
        controller?.dispose();
      }
    }());
  }

  @override
  void dispose() {
    _releaseForm();
    _text.dispose();
    super.dispose();
  }

  Future<void> _closeForm() async {
    final controller = _controller;
    final transport = _transport ?? _unclosed;
    if (transport == null) return;
    try {
      if (controller != null) {
        await controller.close();
        await transport.close();
      } else {
        // A new explicit user action may retry the idempotent close request.
        await transport.retryClose();
      }
      if (identical(_unclosed, transport)) _unclosed = null;
    } catch (_) {
      _unclosed = transport;
      rethrow;
    } finally {
      controller?.dispose();
      if (identical(_controller, controller)) {
        _controller = null;
        _transport = null;
        _formId = null;
        if (mounted) setState(() {});
      }
    }
  }

  Future<void> _loadPages(ExternalPluginControl backend) async {
    var cursor = '';
    BigInt? revision;
    final entries = <PluginLibraryEntry>[];
    final cursors = <String>{};
    final ids = <String>{};
    do {
      if (!cursors.add(cursor) || cursors.length > 4096) {
        throw const FormatException('插件分页未能结束');
      }
      final page = await backend.pluginPage(cursor: cursor, revision: revision);
      if (!_current(backend)) return;
      revision ??= page.revision;
      if (page.revision != revision) throw const FormatException('插件列表已变化');
      for (final entry in page.entries) {
        if (!ids.add(entry.id)) throw const FormatException('插件列表包含重复条目');
        entries.add(entry);
      }
      cursor = page.cursor;
    } while (cursor.isNotEmpty);
    if (!_current(backend)) return;
    setState(() {
      _entries = entries;
      _revision = revision;
      _confirmed = true;
      _clearTool();
      _approvals.clear();
      for (final entry in entries) {
        _approvals[entry.id] = entry.approved.toSet();
      }
      _preview = null;
      _candidatePath = null;
    });
  }

  Future<void> _guard(
    Future<void> Function(ExternalPluginControl) action,
    String failure,
  ) async {
    if (_busy) return;
    final backend = widget.backend;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      await action(backend);
    } catch (_) {
      if (!_current(backend)) return;
      setState(() {
        _confirmed = false;
        _message = '$failure。操作未确认，请核对刷新后的状态再选择。';
      });
      try {
        await _loadPages(backend);
      } catch (_) {
        if (_current(backend)) {
          setState(() => _message = '$failure。列表暂未刷新，请点“刷新列表”重试读取。');
        }
      }
      if (_current(backend)) widget.onChanged();
    } finally {
      if (_current(backend)) setState(() => _busy = false);
    }
  }

  Future<void> _refresh() => _guard((backend) async {
    await _closeForm();
    if (!_current(backend)) return;
    setState(() {
      _confirmed = false;
      _clearTool();
    });
    await _loadPages(backend);
  }, '插件列表未能确认');

  Future<String?> _pickPackage() async {
    final selected = await openFile(
      acceptedTypeGroups: [
        const XTypeGroup(
          label: 'Morrow 插件',
          extensions: ['mplugin', 'morrowplugin'],
        ),
      ],
    );
    return selected?.path;
  }

  Future<void> _inspect() => _guard((backend) async {
    final path = await (widget.pickPackage ?? _pickPackage)();
    if (path == null || !_current(backend)) return;
    final preview = await backend.inspectPlugin(path);
    if (!_current(backend)) return;
    if (preview.entries.length != 1) throw const FormatException('插件预览不完整');
    setState(() {
      _candidatePath = path;
      _preview = preview;
    });
  }, '插件预览未能读取');
  Future<void> _import() => _guard((backend) async {
    final preview = _preview;
    final path = _candidatePath;
    if (preview == null || path == null) return;
    await _closeForm();
    if (!_current(backend)) return;
    await backend.importPlugin(
      path,
      Uint8List.fromList(preview.entries.single.digest),
      preview.revision,
    );
    if (!_current(backend)) return;
    _clearTool();
    await _loadPages(backend);
    if (!_current(backend)) return;
    setState(
      () => _message =
          _entries.any(
            (entry) => entry.id == preview.entries.single.id && entry.enabled,
          )
          ? '此版本已在插件列表中，现有启用状态保持不变。'
          : '已导入，尚未启用。请选择需要允许的权限。',
    );
    widget.onChanged();
  }, '导入结果未能确认');
  Future<void> _configure(PluginLibraryEntry entry, bool enable) =>
      _guard((backend) async {
        final revision = _revision!;
        final approval = enable
            ? (_approvals[entry.id] ?? {}).toList()
            : entry.approved;
        await _closeForm();
        if (!_current(backend)) return;
        await backend.configureExternal(entry, revision, approval, enable);
        if (!_current(backend)) return;
        _clearTool();
        await _loadPages(backend);
        if (_current(backend)) widget.onChanged();
      }, '启用或权限变更未能确认');
  Future<void> _remove(PluginLibraryEntry entry) => _guard((backend) async {
    final revision = _revision!;
    await _closeForm();
    if (!_current(backend)) return;
    await backend.removeExternal(entry, revision);
    if (!_current(backend)) return;
    _clearTool();
    await _loadPages(backend);
    if (!_current(backend)) return;
    setState(() => _message = '已卸载，已有内容仍保留。');
    widget.onChanged();
  }, '卸载结果未能确认');

  bool _standardUi(PluginLibraryEntry entry) =>
      entry.handlers.any(
        (h) =>
            h.name == 'ui.form' &&
            h.inputType == 'text.utf8' &&
            h.outputType == 'morrow.ui.document.v1' &&
            h.maxInputBytes >= 32 &&
            h.maxOutputBytes == 65536,
      ) &&
      entry.handlers.any(
        (h) =>
            h.name == 'ui.edit' &&
            h.inputType == 'morrow.ui.event.v1' &&
            h.outputType == 'morrow.ui.document.v1' &&
            h.maxInputBytes >= 65536 &&
            h.maxOutputBytes == 65536,
      );
  List<PluginTransformHandler> _transforms(PluginLibraryEntry entry) => entry
      .handlers
      .where((h) => h.name != 'ui.form' && h.name != 'ui.edit')
      .toList();
  Future<void> _openForm(PluginLibraryEntry entry) => _guard((backend) async {
    final revision = _revision!;
    await _closeForm();
    if (!_current(backend)) return;
    final transport = _ObservedTransport(
      backend.createExternalPluginUi(entry, revision),
    );
    final controller = PluginUiController(transport);
    setState(() {
      _transport = transport;
      _controller = controller;
      _formId = entry.id;
    });
    await controller.open('');
    if (_current(backend) && controller.failure != null) {
      setState(() => _message = controller.failure!.message);
    }
  }, '插件界面未能打开');
  void _clearTool() {
    _toolId = null;
    _handler = null;
    _fileBytes = null;
    _fileName = null;
    _result = null;
  }

  void _selectTool(PluginLibraryEntry entry) {
    setState(() {
      _clearTool();
      _toolId = entry.id;
      _handler = _transforms(entry).first;
      _text.clear();
    });
  }

  Future<void> _pickInput() => _guard((backend) async {
    final handler = _handler;
    if (handler == null) return;
    final file = await openFile();
    if (file == null || !_current(backend)) return;
    final limit = handler.maxInputBytes.clamp(0, 65536).toInt();
    if (await file.length() > limit) {
      if (_current(backend)) {
        setState(() => _message = '文件太大，请选择不超过 $limit 字节的文件。');
      }
      return;
    }
    final builder = BytesBuilder();
    await for (final chunk in file.openRead(0, limit + 1)) {
      if (builder.length + chunk.length > limit) {
        throw const FormatException('文件超出限制');
      }
      builder.add(chunk);
    }
    if (_current(backend)) {
      setState(() {
        _fileBytes = builder.takeBytes();
        _fileName = file.name;
        _result = null;
      });
    }
  }, '输入文件未能读取');
  Future<void> _transform(PluginLibraryEntry entry) => _guard((backend) async {
    final handler = _handler;
    final revision = _revision;
    if (handler == null || revision == null) return;
    final input = _fileBytes ?? Uint8List.fromList(utf8.encode(_text.text));
    if (input.length > handler.maxInputBytes || input.length > 65536) {
      setState(() => _message = '输入太长，请缩短文字或选择更小的文件。');
      return;
    }
    setState(() => _result = null);
    final bytes = await backend.transformExternal(
      entry,
      revision,
      handler,
      Uint8List.fromList(input),
    );
    if (!_current(backend)) return;
    if (bytes.length > handler.maxOutputBytes || bytes.length > 65536) {
      throw const FormatException('输出超出限制');
    }
    String preview;
    try {
      final text = utf8.decode(bytes);
      if (text.runes.any(
        (value) => value < 32 && value != 9 && value != 10 && value != 13,
      )) {
        throw const FormatException('非文本内容');
      }
      preview = String.fromCharCodes(text.runes.take(4096));
      if (text.runes.length > 4096) preview += '\n…仅显示前 4096 个字符';
      if (text.isEmpty) preview = '（空结果）';
    } on FormatException {
      preview =
          '二进制内容：${bytes.take(64).map((b) => b.toRadixString(16).padLeft(2, '0')).join(' ')}${bytes.length > 64 ? ' …' : ''}';
    }
    setState(() => _result = '${bytes.length} 字节\n$preview');
  }, '转换结果未能确认');

  String _capability(String name) =>
      const {
        'summary': '读取摘要',
        'operation': '查询操作结果',
        'attachment': '读取附件',
        'create-content': '新建内容',
        'edit-content': '编辑内容',
        'read-content': '读取内容',
        'createContent': '新建内容',
        'editContent': '编辑内容',
        'readContent': '读取内容',
        'rename': '重命名',
        'readSummary': '读取摘要',
        'queryOperation': '查询操作结果',
        'readAttachment': '读取附件',
        'CreateContent': '新建内容',
        'EditContent': '编辑内容',
        'ReadContent': '读取内容',
        'Rename': '重命名',
        'ReadSummary': '读取摘要',
        'QueryOperation': '查询操作结果',
        'ReadAttachment': '读取附件',
      }[name] ??
      '其他声明权限：$name';
  Widget _note(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
    ),
  );
  Widget _button(
    String label,
    String key,
    VoidCallback? action, {
    IconData icon = Icons.chevron_right,
  }) => OutlinedButton.icon(
    key: ValueKey(key),
    onPressed: _busy ? null : action,
    style: OutlinedButton.styleFrom(
      foregroundColor: widget.ink,
      minimumSize: const Size(0, 38),
      side: BorderSide(color: widget.line),
      shape: RoundedRectangleBorder(borderRadius: widget.radius),
    ),
    icon: Icon(icon, size: 16),
    label: Text(label, style: const TextStyle(fontSize: 11)),
  );
  Widget _facts(PluginLibraryEntry entry) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      _note('${entry.id} · ${entry.version}'),
      _note(
        entry.declared.isEmpty
            ? '未声明内容权限。'
            : '声明权限：${entry.declared.map(_capability).join('、')}',
      ),
      if (entry.dependencies.isNotEmpty) ...[
        _note('声明了依赖，需要在宿主配置。本页不会批准依赖。'),
        ...entry.dependencies.map(_note),
      ],
      if (entry.issue.isNotEmpty) _note(entry.issue),
    ],
  );
  Widget _entry(PluginLibraryEntry entry) {
    final usable = _confirmed && entry.available && entry.enabled;
    return ExpansionTile(
      key: ValueKey('plugin-entry-${entry.id}'),
      tilePadding: EdgeInsets.zero,
      childrenPadding: const EdgeInsets.only(bottom: 12),
      title: Text(
        entry.name,
        style: TextStyle(color: widget.ink, fontSize: 13),
      ),
      subtitle: Text(
        entry.builtin
            ? '内置工作台'
            : entry.enabled
            ? '已启用'
            : '未启用',
        style: TextStyle(color: widget.muted, fontSize: 11),
      ),
      children: [
        _facts(entry),
        if (entry.builtin)
          _note('请使用上方工作台插件按钮管理此插件。')
        else ...[
          ...entry.declared.map(
            (cap) => CheckboxListTile(
              key: ValueKey('plugin-cap-${entry.id}-$cap'),
              dense: true,
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              title: Text(
                _capability(cap),
                style: TextStyle(color: widget.ink, fontSize: 12),
              ),
              value: _approvals[entry.id]?.contains(cap) ?? false,
              onChanged: _busy || !_confirmed
                  ? null
                  : (checked) => setState(() {
                      final selected = _approvals.putIfAbsent(
                        entry.id,
                        () => {},
                      );
                      checked == true
                          ? selected.add(cap)
                          : selected.remove(cap);
                    }),
            ),
          ),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _button(
                entry.enabled ? '保存权限' : '批准并启用',
                'plugin-approve-${entry.id}',
                !_confirmed || !entry.available
                    ? null
                    : () => _configure(entry, true),
                icon: Icons.check_circle_outline,
              ),
              if (entry.enabled)
                _button(
                  '停用',
                  'plugin-disable-${entry.id}',
                  !_confirmed ? null : () => _configure(entry, false),
                  icon: Icons.pause_circle_outline,
                ),
              _button(
                '卸载（保留内容）',
                'plugin-remove-${entry.id}',
                !_confirmed ? null : () => _remove(entry),
                icon: Icons.remove_circle_outline,
              ),
              if (_transforms(entry).isNotEmpty)
                _button(
                  '使用转换',
                  'plugin-transform-${entry.id}',
                  !usable ? null : () => _selectTool(entry),
                  icon: Icons.auto_fix_high_outlined,
                ),
              if (_standardUi(entry))
                _button(
                  _formId == entry.id ? '关闭界面' : '打开界面',
                  'plugin-ui-${entry.id}',
                  !usable
                      ? null
                      : () {
                          if (_formId == entry.id) {
                            unawaited(
                              _guard((_) => _closeForm(), '插件界面关闭未能确认'),
                            );
                          } else {
                            unawaited(_openForm(entry));
                          }
                        },
                  icon: Icons.view_quilt_outlined,
                ),
            ],
          ),
          if (_toolId == entry.id) ...[
            const SizedBox(height: 12),
            DropdownButton<PluginTransformHandler>(
              key: const ValueKey('plugin-handler'),
              isExpanded: true,
              value: _handler,
              items: _transforms(entry)
                  .map(
                    (handler) => DropdownMenuItem(
                      value: handler,
                      child: Text(
                        handler.name,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  )
                  .toList(),
              onChanged: _busy
                  ? null
                  : (handler) => setState(() {
                      _handler = handler;
                      _result = null;
                      _fileBytes = null;
                      _fileName = null;
                    }),
            ),
            TextField(
              key: const ValueKey('plugin-input'),
              controller: _text,
              enabled: !_busy && _fileBytes == null,
              minLines: 2,
              maxLines: 5,
              decoration: const InputDecoration(labelText: '输入文字'),
              onChanged: (_) => setState(() => _result = null),
            ),
            if (_fileName != null) _note('已选文件：$_fileName'),
            Wrap(
              spacing: 8,
              children: [
                _button(
                  '选择小文件',
                  'plugin-input-file',
                  _pickInput,
                  icon: Icons.attach_file,
                ),
                if (_fileBytes != null)
                  _button(
                    '改用文字',
                    'plugin-input-text',
                    () => setState(() {
                      _fileBytes = null;
                      _fileName = null;
                      _result = null;
                    }),
                  ),
                _button('转换', 'plugin-run', () => _transform(entry)),
              ],
            ),
            if (_result != null)
              SelectableText(
                _result!,
                key: const ValueKey('plugin-result'),
                style: TextStyle(color: widget.ink, fontSize: 12),
              ),
            _note('结果仅供预览，不会自动写入已有内容。'),
          ],
          if (_formId == entry.id && _controller != null)
            Padding(
              padding: const EdgeInsets.only(top: 12),
              child: ManagedPluginForm(controller: _controller!),
            ),
        ],
      ],
    );
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(19),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          '第三方插件',
          style: TextStyle(
            color: widget.ink,
            fontSize: 13,
            fontWeight: FontWeight.w600,
          ),
        ),
        _note('导入后由你选择是否启用。停用或卸载不会删除已有内容。'),
        Wrap(
          spacing: 8,
          children: [
            _button('选择插件文件', 'plugin-pick', _inspect, icon: Icons.add),
            _button('刷新列表', 'plugin-refresh', _refresh, icon: Icons.refresh),
          ],
        ),
        if (_busy)
          const Padding(
            padding: EdgeInsets.symmetric(vertical: 8),
            child: LinearProgressIndicator(),
          ),
        if (_message != null) _note(_message!),
        if (_preview != null)
          Container(
            key: const ValueKey('plugin-preview'),
            margin: const EdgeInsets.symmetric(vertical: 12),
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              border: Border.all(color: widget.line),
              borderRadius: widget.radius,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '导入预览：${_preview!.entries.single.name}',
                  style: TextStyle(color: widget.ink),
                ),
                _facts(_preview!.entries.single),
                _note('目前仅检查了文件。导入后仍需单独启用。'),
                Wrap(
                  spacing: 8,
                  children: [
                    _button('导入', 'plugin-import', _import),
                    _button(
                      '取消',
                      'plugin-cancel-import',
                      () => setState(() {
                        _preview = null;
                        _candidatePath = null;
                      }),
                    ),
                  ],
                ),
              ],
            ),
          ),
        if (_confirmed && _entries.isEmpty) _note('尚未导入第三方插件。'),
        ..._entries.map(_entry),
      ],
    ),
  );
}
