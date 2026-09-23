import 'versioned_content.dart';

final _maxU64 = (BigInt.one << 64) - BigInt.one;

/// One display row. A legacy row has no TaskId; a format-2 row always does.
sealed class VersionedIdeaTaskView {
  const VersionedIdeaTaskView(this.text);
  final String text;
}

final class LegacyIdeaTaskView extends VersionedIdeaTaskView {
  const LegacyIdeaTaskView({
    required String text,
    required this.completedByName,
    required this.duplicateCount,
  }) : super(text);

  /// Format 1 completion is membership by text, including duplicate labels.
  final bool completedByName;
  final int duplicateCount;
}

final class IdentifiedIdeaTaskView extends VersionedIdeaTaskView {
  const IdentifiedIdeaTaskView({
    required this.taskId,
    required String text,
    required this.completion,
    required this.legacyCompleted,
    required this.legacyDuplicates,
  }) : super(text);

  final String taskId;
  final VersionedTaskCompletion completion;
  final bool legacyCompleted;
  final int legacyDuplicates;
  bool get needsExplicitDecision =>
      completion == VersionedTaskCompletion.legacyAmbiguous;
}

/// An immutable read projection for mixed-format cards. It never converts a
/// format-2 task into Idea.todos/completed or narrows u64 asset lengths to int.
final class VersionedIdeaView {
  VersionedIdeaView._(this.source, this.tasks);

  factory VersionedIdeaView.fromRecord(VersionedContentRecord source) {
    if (source.id.isEmpty ||
        source.revision <= BigInt.zero ||
        source.revision > _maxU64 ||
        source.deletedAt < BigInt.zero ||
        source.deletedAt > _maxU64 ||
        (source.deleted && source.deletedAt == BigInt.zero) ||
        source.completeCount < 0 ||
        source.incompleteCount < 0 ||
        source.ambiguousCount < 0 ||
        source.icon < 0 ||
        source.icon > 0xffff ||
        source.color < 0 ||
        source.color > 0xffffffff) {
      throw const FormatException('Invalid versioned card display metadata');
    }
    final assetIds = <String>{};
    for (final asset in source.assets) {
      if (asset.id.isEmpty ||
          asset.name.isEmpty ||
          asset.kind.isEmpty ||
          !assetIds.add(asset.id) ||
          asset.bytes < BigInt.zero ||
          asset.bytes > _maxU64) {
        throw const FormatException('Invalid versioned card asset');
      }
    }
    if (source.formatVersion == 1) {
      if (source.tasks.isNotEmpty ||
          source.origin != null ||
          source.retiredTaskIds.isNotEmpty ||
          source.ambiguousCount != 0 ||
          source.completeCount + source.incompleteCount !=
              source.todos.length) {
        throw const FormatException('Invalid legacy card display shape');
      }
      final duplicates = <String, int>{};
      for (final text in source.todos) {
        duplicates[text] = (duplicates[text] ?? 0) + 1;
      }
      return VersionedIdeaView._(
        source,
        List.unmodifiable([
          for (final text in source.todos)
            LegacyIdeaTaskView(
              text: text,
              completedByName: source.completed.contains(text),
              duplicateCount: duplicates[text]!,
            ),
        ]),
      );
    }
    if (source.formatVersion == 2) {
      if (source.todos.isNotEmpty ||
          source.completed.isNotEmpty ||
          source.completeCount +
                  source.incompleteCount +
                  source.ambiguousCount !=
              source.tasks.length) {
        throw const FormatException('Invalid TaskId card display shape');
      }
      final taskIds = <String>{};
      for (final task in source.tasks) {
        if (task.id.isEmpty || !taskIds.add(task.id)) {
          throw const FormatException('Invalid display TaskId');
        }
      }
      return VersionedIdeaView._(
        source,
        List.unmodifiable([
          for (final task in source.tasks)
            IdentifiedIdeaTaskView(
              taskId: task.id,
              text: task.text,
              completion: task.completion,
              legacyCompleted: task.legacyCompleted,
              legacyDuplicates: task.legacyDuplicates,
            ),
        ]),
      );
    }
    throw const FormatException('Unsupported card display format');
  }

  final VersionedContentRecord source;
  final List<VersionedIdeaTaskView> tasks;

  String get id => source.id;
  int get formatVersion => source.formatVersion;
  BigInt get revision => source.revision;
  bool get deleted => source.deleted;
  BigInt get deletedAt => source.deletedAt;
  String get title => source.title;
  String get description => source.description;
  String get category => source.category;
  String get stage => source.stage;
  String get projectedStage => source.projectedStage;
  String get hypothesis => source.hypothesis;
  String get conclusion => source.conclusion;
  bool get favorite => source.favorite;
  int get iconIndex => source.icon;
  int get colorArgb => source.color;
  List<VersionedAsset> get assets => source.assets;
  VersionedOrigin? get origin => source.origin;
  List<String> get retiredTaskIds => source.retiredTaskIds;
  int get completeCount => source.completeCount;
  int get incompleteCount => source.incompleteCount;
  int get ambiguousCount => source.ambiguousCount;
}
