import 'studio_backend.dart';
import '../main.dart' show Idea;

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

/// Flutter owns the visible projection. Implementations commit before returning.
abstract class WorkbenchBackend {
  bool get writable;
  StudioBackend? get studio => null;
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  });
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort,
  );
}

/// Optional trusted-host capability, independent of business plugin availability.
abstract interface class WorkbenchProtectionBackup {
  Future<void> backupProtection(String destination);
  Future<void> backupSnapshot(String destination);
}
