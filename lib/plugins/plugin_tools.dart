import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:morrow_plugin_ui/online.dart';

class PluginManagementState {
  const PluginManagementState({
    required this.revision,
    required this.digest,
    required this.enabled,
    required this.approved,
    required this.available,
    required this.writable,
  });
  final BigInt revision;
  final Uint8List digest;
  final bool enabled, approved, available, writable;
}

abstract interface class WorkbenchPluginControl {
  Future<PluginManagementState> pluginState();
  Future<PluginManagementState> configurePlugin(
    PluginManagementState expected,
    bool enable,
  );
  PluginUiTransport createPluginUi();
}

class PluginTools extends StatefulWidget {
  const PluginTools({
    super.key,
    required this.backend,
    required this.onChanged,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
  });
  final WorkbenchPluginControl backend;
  final VoidCallback onChanged;
  final Color ink, muted, line;
  final BorderRadius radius;
  @override
  State<PluginTools> createState() => _PluginToolsState();
}

class _PluginToolsState extends State<PluginTools> {
  PluginManagementState? _state;
  PluginUiController? _controller;
  bool _busy = false;
  String? _message;
  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final value = await widget.backend.pluginState();
      if (mounted) {
        setState(() {
          _state = value;
          _message = null;
        });
        widget.onChanged();
      }
    } catch (_) {
      if (mounted) setState(() => _message = '插件状态暂时无法读取，请重试。');
    }
  }

  Future<void> _closeTool() async {
    final controller = _controller;
    if (mounted) setState(() => _controller = null);
    if (controller != null) {
      await controller.close();
      controller.dispose();
    }
  }

  Future<void> _configure() async {
    final state = _state;
    if (_busy || state == null) return;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      await _closeTool();
      final changed = await widget.backend.configurePlugin(
        state,
        !state.enabled,
      );
      if (mounted) {
        setState(() => _state = changed);
        widget.onChanged();
      }
    } catch (_) {
      if (mounted) setState(() => _message = '插件设置未确认，请刷新状态后重新选择。');
      await _load();
      if (mounted) widget.onChanged();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _openTool() async {
    if (_busy || _controller != null) return;
    final controller = PluginUiController(widget.backend.createPluginUi());
    setState(() {
      _controller = controller;
      _message = null;
    });
    await controller.open('');
  }

  @override
  void dispose() {
    _controller?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = _state;
    final style = OutlinedButton.styleFrom(
      foregroundColor: widget.ink,
      minimumSize: const Size.fromHeight(38),
      side: BorderSide(color: widget.line),
      shape: RoundedRectangleBorder(borderRadius: widget.radius),
    );
    return Padding(
      padding: const EdgeInsets.all(19),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.extension_outlined, size: 16, color: widget.ink),
              const SizedBox(width: 8),
              Text(
                '工作台插件',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: widget.ink,
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Text(
            state == null
                ? '正在读取插件状态…'
                : !state.available
                ? '插件管理暂不可用，已有内容仍可读取。'
                : state.enabled && !state.approved
                ? '插件已启用，但内容权限不足；工作台保持只读。可停用后重新确认权限。'
                : state.enabled && !state.writable
                ? '插件已允许，但当前工作台只读；处理内容库或插件提示后可刷新状态。'
                : state.enabled
                ? '已启用。插件可读取和编辑工作台内容，停用后保留已有资料。'
                : '尚未启用。允许读取和编辑工作台内容后，可继续使用编辑与工具功能。',
            style: TextStyle(fontSize: 11, height: 1.6, color: widget.muted),
          ),
          const SizedBox(height: 12),
          if (state?.available == true)
            OutlinedButton.icon(
              key: const ValueKey('plugin-enable'),
              onPressed: _busy ? null : _configure,
              style: style,
              icon: Icon(
                state!.enabled
                    ? Icons.pause_circle_outline
                    : Icons.check_circle_outline,
                size: 16,
              ),
              label: Text(
                state.enabled ? '停用工作台插件' : '允许读取与编辑，并启用',
                style: const TextStyle(fontSize: 11),
              ),
            ),
          if (state?.enabled == true) ...[
            const SizedBox(height: 8),
            OutlinedButton.icon(
              key: const ValueKey('plugin-text-tool'),
              onPressed: _busy
                  ? null
                  : _controller == null
                  ? _openTool
                  : _closeTool,
              style: style,
              icon: const Icon(Icons.text_fields, size: 16),
              label: Text(
                _controller == null ? '打开文字工具' : '收起文字工具',
                style: const TextStyle(fontSize: 11),
              ),
            ),
          ],
          if (_controller != null) ...[
            const SizedBox(height: 12),
            ManagedPluginForm(controller: _controller!),
          ],
          if (_message != null) ...[
            const SizedBox(height: 10),
            Text(
              _message!,
              style: TextStyle(fontSize: 11, color: widget.muted),
            ),
          ],
          TextButton(
            onPressed: _busy ? null : _load,
            child: const Text('刷新状态'),
          ),
        ],
      ),
    );
  }
}
