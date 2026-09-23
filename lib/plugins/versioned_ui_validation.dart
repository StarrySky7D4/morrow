import 'dart:convert';
import 'versioned_content.dart';

/// Local input constraints only, never authority to commit or proof that an
/// already-sent operation did not execute. The guest still validates all input.
void validateVersionedTaskDraft(
  VersionedContentRecord source,
  TaskEditCommand command,
) {
  void require(bool valid) {
    if (!valid) throw const VersionedMutationNotSubmitted();
  }

  require(source.formatVersion == 2 && !source.deleted);
  final ids = source.tasks.map((task) => task.id).toSet();
  switch (command.kind) {
    case TaskEditKind.rename:
      require(ids.contains(command.taskId));
      require(
        command.text.isNotEmpty && utf8.encode(command.text).length <= 2048,
      );
    case TaskEditKind.add:
      require(
        command.text.isNotEmpty && utf8.encode(command.text).length <= 2048,
      );
      require(
        source.tasks.length < 128 &&
            command.taskId.isNotEmpty &&
            utf8.encode(command.taskId).length <= 256 &&
            !command.taskId.runes.any(
              (value) =>
                  value < 32 ||
                  (value >= 127 && value <= 159) ||
                  const [47, 92, 58].contains(value),
            ) &&
            !ids.contains(command.taskId) &&
            !source.retiredTaskIds.contains(command.taskId),
      );
    case TaskEditKind.remove:
      require(
        ids.contains(command.taskId) && source.retiredTaskIds.length < 4096,
      );
    case TaskEditKind.setCompletion:
      require(ids.contains(command.taskId));
    case TaskEditKind.reorder:
      require(
        command.order.length == ids.length &&
            command.order.toSet().length == ids.length &&
            command.order.every(ids.contains),
      );
    case TaskEditKind.setStage:
    case TaskEditKind.completeAllAndSetStage:
      final stages = switch (source.category) {
        '灵感' => const ['待整理', '已整理'],
        '进行中' => const ['计划中', '推进中', '已完成'],
        '实验' => const ['待验证', '验证中', '已记录'],
        _ => const <String>[],
      };
      require(stages.contains(command.text));
  }
}
