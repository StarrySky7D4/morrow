import 'versioned_content.dart';
import 'dart:math';
import 'studio_backend.dart';
import '../main.dart' show Idea;

/// Cap'n Proto carries u64 revisions through a signed Dart int.
BigInt unsignedContentRevision(int signedCarrier) =>
    BigInt.from(signedCarrier).toUnsigned(64);

/// The host confirmed the operation, but a current card could not be read.
/// The original operation remains the only safe retry for a captured editor.
class WorkbenchCommittedRefreshFailure implements Exception {
  const WorkbenchCommittedRefreshFailure(this.cause);
  final Object cause;
  @override
  String toString() => 'Commit confirmed; current content refresh failed';
}

/// Session-local version knowledge from the native host. Values are unsigned u64.
abstract interface class WorkbenchContentRevisionSource {
  Iterable<String> knownContentIds();
  BigInt? knownContentRevision(String id);
  bool? knownContentDeleted(String id);
}

/// A mutation failure may be an unknown transport outcome. Only a read-only
/// refresh is safe from a generic UI retry button.
abstract interface class WorkbenchMutationFailureNeedsRefresh {}

/// Query operation identity is independent of transport serials and content revisions.
String newQueryOperationId() {
  final random = Random.secure();
  final bytes = List.generate(
    16,
    (_) => random.nextInt(256).toRadixString(16).padLeft(2, '0'),
  );
  return 'query-${bytes.join()}';
}

/// Only a typed host terminal response permits replacing an uncertain operation.
class QueryFailure implements Exception {
  const QueryFailure(
    this.message, {
    required this.terminal,
    this.capacity = false,
  });
  final String message;
  final bool terminal;
  final bool capacity;
  @override
  String toString() => message;
}

enum PluginAction {
  create,
  edit,
  favorite,
  todo,
  stage,
  toProject,
  delete,
  restore,
}

/// Flutter owns the visible projection. Mutations commit before returning.
abstract class WorkbenchBackend {
  bool get writable;
  StudioBackend? get studio => null;
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  });

  /// Reuse an operation only for the identical intent. A stored ready result is
  /// computation evidence; only a received response can update this UI.
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  });
}

/// Optional trusted-host capability, independent of business plugin availability.
abstract interface class WorkbenchProtectionBackup {
  Future<void> backupProtection(String destination);
  Future<void> backupSnapshot(String destination);
}

/// Native mixed-format presentation. The records remain owned by the core;
/// these Ideas are transient views and never a legacy persistence payload.
abstract interface class WorkbenchMixedContent {
  Future<List<Idea>> loadWorkspaceContent();
  Future<Idea> workspaceRecord(VersionedContentRecord record);
  Future<Idea> applyWorkspaceCard(
    String operation,
    Idea idea,
    CardEditCommand command,
  );
  Future<Idea> applyWorkspaceTask(
    String operation,
    Idea idea,
    TaskEditCommand command,
  );
}
