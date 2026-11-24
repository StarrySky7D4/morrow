import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';

import 'card_changes_watch.dart';
import 'channel_task_models.dart';

final class ChangesWatchControls extends StatefulWidget {
  const ChangesWatchControls({
    super.key,
    required this.backend,
    required this.packageId,
    required this.digest,
    required this.revision,
    required this.readSelection,
    required this.enabled,
  });
  final ChangesObservationBackend backend;
  final String packageId;
  final Uint8List digest;
  final BigInt revision;
  final ChangesWindowReview Function() readSelection;
  final bool enabled;
  @override
  State<ChangesWatchControls> createState() => _ChangesWatchControlsState();
}

final class _ChangesWatchControlsState extends State<ChangesWatchControls> {
  bool reviewing = false;
  Object? formError;
  String t(String en, String zh) =>
      Localizations.localeOf(context).languageCode == 'zh' ? zh : en;

  Future<void> begin() async {
    if (reviewing || !widget.enabled) return;
    final original = widget;
    setState(() {
      reviewing = true;
      formError = null;
    });
    try {
      final selected = original.readSelection();
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(t('Watch these cards?', '持续观察这些卡片？')),
          content: SingleChildScrollView(
            child: Text(
              [
                original.packageId,
                selected.cards.join('\n'),
                t(
                  'For up to one hour, this workbench will check the confirmed cursor and whether these cards have new changes. You can close this dialog and edit cards. Each plugin page needs a separate approval.',
                  '工作台将在一小时内核对游标和这些卡片的新变更。关闭此窗口后仍可编辑卡片。插件每次读取一页都需要单独批准。',
                ),
              ].join('\n\n'),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(t('Cancel', '取消')),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: Text(t('Start watching', '开始观察')),
            ),
          ],
        ),
      );
      if (confirmed != true ||
          !mounted ||
          !identical(widget.backend, original.backend) ||
          widget.packageId != original.packageId ||
          widget.revision != original.revision ||
          !ChannelValidation.same(widget.digest, original.digest))
        return;
      await CardChangesWatch.forBackend(
        original.backend,
      ).start(original.packageId, original.digest, original.revision, selected);
    } catch (error) {
      if (mounted) setState(() => formError = error);
    } finally {
      if (mounted) setState(() => reviewing = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final watch = CardChangesWatch.forBackend(widget.backend);
    return ListenableBuilder(
      listenable: watch,
      builder: (context, child) {
        final here = watch.matches(
          widget.packageId,
          widget.digest,
          widget.revision,
        );
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (watch.active) ...[
              Text(
                here
                    ? '${t('Watching', '正在观察')} ${watch.selection!.cards.join(', ')}'
                    : t(
                        'A watch is active for another selection.',
                        '工作台正在观察另一组卡片。',
                      ),
              ),
              if (here)
                Text(switch (watch.progress?.pending) {
                  true => t(
                    'Changes are available for this selection.',
                    '这些卡片有待读取的变更。',
                  ),
                  false => t('Caught up at the last check.', '上次核对时已读完。'),
                  null => t('Waiting for the original library.', '正在等待原资料库。'),
                }),
              if (watch.progress?.latest case final Uint8List cursor when here)
                SelectableText(
                  cursor
                      .map((value) => value.toRadixString(16).padLeft(2, '0'))
                      .join(),
                ),
              TextButton(
                onPressed: watch.stop,
                child: Text(t('Stop watching', '停止观察')),
              ),
            ] else ...[
              if (here && watch.expired)
                Text(t('The one-hour watch ended.', '一小时的观察已结束。')),
              if (here && watch.failure != null)
                Text(
                  t(
                    'The original selection could no longer be verified.',
                    '已无法核实原先选择的卡片。',
                  ),
                ),
              TextButton(
                onPressed: widget.enabled && !reviewing ? begin : null,
                child: Text(t('Watch these cards…', '持续观察这些卡片…')),
              ),
            ],
            if (formError != null) Text(formError.toString()),
          ],
        );
      },
    );
  }
}
