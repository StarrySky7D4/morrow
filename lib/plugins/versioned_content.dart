/// Typed read and mutation surface for idea formats 1 and 2.
/// A mutation receipt is historical evidence; [VersionedMutationResult.current]
/// is a separately refreshed, authoritative current record.
abstract interface class WorkbenchVersionedContent {
  VersionedContentControl get versionedContent;
}

abstract interface class VersionedContentControl {
  Future<VersionedContentRecord> read(String id);
  Future<VersionedContentPage> page({String cursor = '', int limit = 128});
  Future<TasksMigrationPlan> planMigration(String id);
  Future<VersionedMutationResult> migrate(TasksMigrationPlan plan);
  Future<VersionedMutationResult> editTasks(
    String operation,
    String id,
    BigInt revision,
    TaskEditCommand command,
  );
  Future<VersionedMutationResult> editCard(
    String operation,
    String id,
    BigInt revision,
    CardEditCommand command,
  );
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    required String operation,
  });
}

final class VersionedContentPage {
  VersionedContentPage({required List<String> ids, required this.nextCursor})
    : ids = List.unmodifiable(ids);
  final List<String> ids;
  final String nextCursor;
}

final class VersionedAsset {
  const VersionedAsset({
    required this.id,
    required this.name,
    required this.kind,
    required this.bytes,
  });
  final String id;
  final String name;
  final String kind;
  final BigInt bytes;
}

enum VersionedTaskCompletion { incomplete, complete, legacyAmbiguous }

final class VersionedTask {
  const VersionedTask({
    required this.id,
    required this.text,
    required this.completion,
    required this.legacyCompleted,
    required this.legacyDuplicates,
  });
  final String id;
  final String text;
  final VersionedTaskCompletion completion;
  final bool legacyCompleted;
  final int legacyDuplicates;
}

final class VersionedTaskMapping {
  const VersionedTaskMapping({required this.sourceIndex, required this.taskId});
  final int sourceIndex;
  final String taskId;
}

final class VersionedOrigin {
  VersionedOrigin({
    required this.cardId,
    required this.sourceRevision,
    required List<int> sourceSha256,
    required this.migratorVersion,
    required this.targetVersion,
    required List<int> originalProperties,
    required List<VersionedTaskMapping> mapping,
    required this.historicalProjectStage,
    required this.originalTitle,
  }) : _sourceSha256 = List.unmodifiable(sourceSha256),
       _originalProperties = List.unmodifiable(originalProperties),
       mapping = List.unmodifiable(mapping);
  final String cardId;
  final BigInt sourceRevision;
  final List<int> _sourceSha256;
  List<int> get sourceSha256 => _sourceSha256;
  final int migratorVersion;
  final int targetVersion;
  final List<int> _originalProperties;
  List<int> get originalProperties => _originalProperties;
  final List<VersionedTaskMapping> mapping;
  final String historicalProjectStage;
  final String originalTitle;
}

final class VersionedContentRecord {
  VersionedContentRecord({
    required this.id,
    required this.title,
    required this.revision,
    required this.formatVersion,
    required this.description,
    required this.category,
    required this.stage,
    required this.hypothesis,
    required this.conclusion,
    required this.favorite,
    required List<VersionedAsset> assets,
    required this.icon,
    required this.color,
    required this.deleted,
    required this.deletedAt,
    required List<String> todos,
    required List<String> completed,
    required List<VersionedTask> tasks,
    required this.origin,
    required List<String> retiredTaskIds,
    required this.projectedStage,
    required this.completeCount,
    required this.incompleteCount,
    required this.ambiguousCount,
  }) : assets = List.unmodifiable(assets),
       todos = List.unmodifiable(todos),
       completed = List.unmodifiable(completed),
       tasks = List.unmodifiable(tasks),
       retiredTaskIds = List.unmodifiable(retiredTaskIds);
  final String id;
  final String title;
  final BigInt revision;
  final int formatVersion;
  final String description;
  final String category;
  final String stage;
  final String hypothesis;
  final String conclusion;
  final bool favorite;
  final List<VersionedAsset> assets;
  final int icon;
  final int color;
  final bool deleted;
  final BigInt deletedAt;
  final List<String> todos;
  final List<String> completed;
  final List<VersionedTask> tasks;
  final VersionedOrigin? origin;
  final List<String> retiredTaskIds;
  final String projectedStage;
  final int completeCount;
  final int incompleteCount;
  final int ambiguousCount;
}

final class TasksMigrationPlan {
  const TasksMigrationPlan({
    required this.id,
    required this.operation,
    required this.sourceRevision,
  });
  final String id;
  final String operation;
  final BigInt sourceRevision;
}

final class VersionedCommitReceipt {
  const VersionedCommitReceipt({
    required this.id,
    required this.operation,
    required this.revision,
    required this.repeated,
  });
  final String id;
  final String operation;
  final BigInt revision;
  final bool repeated;
}

final class VersionedMutationResult {
  const VersionedMutationResult({required this.receipt, required this.current});
  final VersionedCommitReceipt receipt;
  final VersionedContentRecord current;
}

enum TaskEditKind {
  setCompletion,
  rename,
  reorder,
  setStage,
  completeAllAndSetStage,
  add,
  remove,
}

final class TaskEditCommand {
  TaskEditCommand._(
    this.kind, {
    this.taskId = '',
    this.text = '',
    this.complete = false,
    List<String> order = const [],
  }) : order = List.unmodifiable(order);
  TaskEditCommand.setCompletion(String taskId, bool complete)
    : this._(TaskEditKind.setCompletion, taskId: taskId, complete: complete);
  TaskEditCommand.rename(String taskId, String text)
    : this._(TaskEditKind.rename, taskId: taskId, text: text);
  TaskEditCommand.reorder(List<String> order)
    : this._(TaskEditKind.reorder, order: order);
  TaskEditCommand.setStage(String stage)
    : this._(TaskEditKind.setStage, text: stage);
  TaskEditCommand.completeAllAndSetStage(String stage)
    : this._(TaskEditKind.completeAllAndSetStage, text: stage);
  TaskEditCommand.add(String taskId, String text)
    : this._(TaskEditKind.add, taskId: taskId, text: text);
  TaskEditCommand.remove(String taskId)
    : this._(TaskEditKind.remove, taskId: taskId);
  final TaskEditKind kind;
  final String taskId;
  final String text;
  final bool complete;
  final List<String> order;
}

final class CardEditFields {
  CardEditFields({
    required this.title,
    required this.description,
    required this.hypothesis,
    required this.conclusion,
    required this.icon,
    required this.color,
    required List<VersionedAsset> assets,
  }) : assets = List.unmodifiable(assets);
  final String title;
  final String description;
  final String hypothesis;
  final String conclusion;
  final int icon;
  final int color;
  final List<VersionedAsset> assets;
}

enum CardEditKind { edit, setFavorite, setCategory, delete, restore }

final class CardEditCommand {
  const CardEditCommand._(
    this.kind, {
    this.fields,
    this.favorite = false,
    this.category = '',
    this.stage = '',
  });
  CardEditCommand.edit(CardEditFields fields)
    : this._(CardEditKind.edit, fields: fields);
  const CardEditCommand.setFavorite(bool favorite)
    : this._(CardEditKind.setFavorite, favorite: favorite);
  const CardEditCommand.setCategory(String category, String stage)
    : this._(CardEditKind.setCategory, category: category, stage: stage);
  const CardEditCommand.delete() : this._(CardEditKind.delete);
  const CardEditCommand.restore() : this._(CardEditKind.restore);
  final CardEditKind kind;
  final CardEditFields? fields;
  final bool favorite;
  final String category;
  final String stage;
}

/// The caller proved this draft was rejected before sending any mutation.
/// Transport failures and generic host error replies must never use this type.
final class VersionedMutationNotSubmitted implements Exception {
  const VersionedMutationNotSubmitted();
  @override
  String toString() => 'Versioned mutation was not submitted';
}

/// The host proved that this exact versioned operation has no commit.
/// A local validation failure cannot provide this proof.
final class VersionedMutationNoCommit implements Exception {
  const VersionedMutationNoCommit({
    required this.id,
    required this.operation,
    required this.sourceRevision,
  });

  final String id;
  final String operation;
  final BigInt sourceRevision;

  @override
  String toString() => 'Versioned mutation has no commit';
}
