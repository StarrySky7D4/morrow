import 'package:flutter/material.dart';

String workbenchFailureMessage(Object error, {String? fallback}) {
  const prefix = 'Morrow workbench host: ';
  final detail = error is StateError ? error.message.toString() : '';
  return detail.startsWith(prefix)
      ? detail.substring(prefix.length)
      : fallback ?? '工作台暂时无法打开，请检查插件文件与数据目录后重试。';
}

/// Recovery actions are host supplied; this view never reads protection files.
class WorkbenchRecovery extends StatefulWidget {
  const WorkbenchRecovery({
    super.key,
    required this.message,
    required this.onRetry,
    this.onRestore,
    this.onRestoreSnapshot,
  });
  final String message;
  final Future<void> Function() onRetry;
  final Future<void> Function()? onRestore;
  final Future<void> Function()? onRestoreSnapshot;
  @override
  State<WorkbenchRecovery> createState() => _WorkbenchRecoveryState();
}

class _WorkbenchRecoveryState extends State<WorkbenchRecovery> {
  bool _busy = false;
  String? _error;
  Future<void> _run(Future<void> Function() action) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = workbenchFailureMessage(error, fallback: '恢复未完成，请保留原文件并重试。');
        });
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = ColorScheme.fromSeed(seedColor: const Color(0xff7468bc));
    return MaterialApp(
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: colors,
        scaffoldBackgroundColor: const Color(0xfff5f4fa),
      ),
      home: Scaffold(
        body: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 440),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: Colors.white.withValues(alpha: .88),
                  borderRadius: BorderRadius.circular(28),
                  border: Border.all(color: Colors.white),
                  boxShadow: [
                    BoxShadow(
                      color: colors.primary.withValues(alpha: .08),
                      blurRadius: 30,
                      offset: const Offset(0, 10),
                    ),
                  ],
                ),
                child: Padding(
                  padding: const EdgeInsets.all(28),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(
                        Icons.lock_reset_rounded,
                        size: 36,
                        color: colors.primary,
                      ),
                      const SizedBox(height: 16),
                      const Text(
                        '重新打开工作台',
                        style: TextStyle(
                          fontSize: 24,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      const SizedBox(height: 12),
                      Text(widget.message, style: const TextStyle(height: 1.6)),
                      if (widget.onRestoreSnapshot != null) ...[
                        const SizedBox(height: 12),
                        const Text(
                          '也可从内容库备份恢复到新目录并切换工作台。原目录会保留；恢复的是备份时的内容，仍需原系统账户。',
                          style: TextStyle(fontSize: 12, height: 1.6),
                        ),
                        const SizedBox(height: 10),
                        OutlinedButton.icon(
                          key: const ValueKey('restore-library-snapshot'),
                          onPressed: _busy
                              ? null
                              : () => _run(widget.onRestoreSnapshot!),
                          icon: const Icon(
                            Icons.inventory_2_outlined,
                            size: 18,
                          ),
                          label: const Text('从内容库备份恢复'),
                        ),
                      ],
                      if (widget.onRestore != null) ...[
                        const SizedBox(height: 12),
                        const Text(
                          '保护文件丢失或损坏时，可选择原文件的备份进行恢复。文件需属于此内容库，并使用原系统账户。',
                          style: TextStyle(
                            height: 1.6,
                            color: Color(0xff686578),
                          ),
                        ),
                      ],
                      if (_error != null) ...[
                        const SizedBox(height: 16),
                        Text(
                          _error!,
                          style: TextStyle(color: colors.error, height: 1.5),
                        ),
                      ],
                      const SizedBox(height: 24),
                      if (_busy)
                        const Padding(
                          padding: EdgeInsets.only(bottom: 16),
                          child: LinearProgressIndicator(),
                        ),
                      Wrap(
                        spacing: 12,
                        runSpacing: 12,
                        children: [
                          FilledButton.tonal(
                            onPressed: _busy
                                ? null
                                : () => _run(widget.onRetry),
                            child: const Text('重试打开'),
                          ),
                          if (widget.onRestore != null)
                            OutlinedButton.icon(
                              onPressed: _busy
                                  ? null
                                  : () => _run(widget.onRestore!),
                              icon: const Icon(Icons.key_rounded, size: 18),
                              label: const Text('选择恢复文件'),
                            ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
