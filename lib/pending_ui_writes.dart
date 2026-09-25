/// Writes already submitted by the UI must finish before its content host gets
/// EOF. Failures remain owned by the caller and its existing recovery protocol.
class PendingUiWrites {
  final _pending = <Future<void>>{};
  Future<T> track<T>(Future<T> operation) {
    late Future<void> settled;
    settled = operation
        .then<void>((_) {}, onError: (Object _, StackTrace _) {})
        .whenComplete(() => _pending.remove(settled));
    _pending.add(settled);
    return operation;
  }

  Future<void> drain() async {
    while (_pending.isNotEmpty) {
      await Future.wait(List<Future<void>>.of(_pending));
    }
  }
}

final pendingUiWrites = PendingUiWrites();
