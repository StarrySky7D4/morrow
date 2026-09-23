/// A fixed observation used to bind an explicit recovery decision. It is not a
/// content snapshot or authority token; the host rechecks the operation.
class SaveRecovery {
  const SaveRecovery({
    required this.operation,
    required this.digest,
    required this.committed,
    required this.conflict,
  });
  final String operation, digest;
  final bool committed, conflict;
}

abstract interface class SaveRecoveryStorage {
  Future<SaveRecovery?> inspectSaveRecovery();
  Future<void> resolveSaveRecovery(
    SaveRecovery observed, {
    required bool abandon,
  });
}
