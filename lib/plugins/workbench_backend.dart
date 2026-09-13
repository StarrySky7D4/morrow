import 'dart:math';
import 'studio_backend.dart';
import '../main.dart' show Idea;

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
