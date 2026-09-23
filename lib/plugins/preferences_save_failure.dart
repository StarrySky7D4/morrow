enum PreferencesEffect { notSubmitted, unknown, locallyCommitted }

/// Public status uses the submission phase and verified receipt, never SQL or
/// exception-string heuristics. Private causes are not rendered by toString.
class PreferencesSaveFailure implements Exception {
  const PreferencesSaveFailure(this.operation, this.effect, this.cause);
  final String operation;
  final PreferencesEffect effect;
  final Object cause;
  @override
  String toString() => 'Preferences submission requires reconciliation';
}
