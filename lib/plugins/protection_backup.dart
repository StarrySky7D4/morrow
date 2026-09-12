import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

class ProtectionBackup extends StatefulWidget {
  const ProtectionBackup({
    super.key,
    required this.onBackup,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.chooseDestination,
  });
  final Future<void> Function(String) onBackup;
  final Future<String?> Function()? chooseDestination;
  final Color ink, muted, line;
  final BorderRadius radius;
  @override
  State<ProtectionBackup> createState() => _ProtectionBackupState();
}

class _ProtectionBackupState extends State<ProtectionBackup> {
  bool _busy = false;
  String? _message;
  bool _failed = false;
  Future<void> _save() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _message = null;
      _failed = false;
    });
    try {
      final String? path;
      if (widget.chooseDestination != null) {
        path = await widget.chooseDestination!();
      } else {
        final stamp = DateTime.now()
            .toIso8601String()
            .replaceAll(':', '-')
            .replaceAll('.', '-');
        path = (await getSaveLocation(
          suggestedName: 'morrow-protection-$stamp.backup',
          acceptedTypeGroups: const [
            XTypeGroup(label: '内容库保护文件', extensions: ['backup']),
          ],
        ))?.path;
      }
      if (path == null || !mounted) return;
      await widget.onBackup(path);
      if (mounted) {
        setState(() {
          _message = '保护文件已备份，可在启动失败时选择此文件恢复。';
        });
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          _failed = true;
          final detail = error is StateError ? error.message.toString() : '';
          _message = detail.isNotEmpty && !detail.contains('内容服务')
              ? detail
              : '备份结果尚未确认，请保留可能生成的文件并检查保存位置。';
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(19),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(Icons.key_rounded, size: 16, color: widget.ink),
            const SizedBox(width: 8),
            Text(
              '内容保护',
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
          '保存原保护文件的备份，供当前系统账户恢复使用。此文件不包含卡片和附件。',
          style: TextStyle(fontSize: 11, height: 1.6, color: widget.muted),
        ),
        const SizedBox(height: 12),
        OutlinedButton.icon(
          key: const ValueKey('backup-protection'),
          onPressed: _busy ? null : _save,
          icon: const Icon(Icons.save_alt_rounded, size: 16),
          label: Text(
            _busy ? '正在备份…' : '备份保护文件',
            style: const TextStyle(fontSize: 11),
          ),
          style: OutlinedButton.styleFrom(
            foregroundColor: widget.ink,
            minimumSize: const Size.fromHeight(38),
            side: BorderSide(color: widget.line),
            shape: RoundedRectangleBorder(borderRadius: widget.radius),
          ),
        ),
        if (_message != null) ...[
          const SizedBox(height: 10),
          Text(
            _message!,
            style: TextStyle(
              fontSize: 11,
              height: 1.6,
              color: _failed
                  ? Theme.of(context).colorScheme.error
                  : widget.muted,
            ),
          ),
        ],
      ],
    ),
  );
}
