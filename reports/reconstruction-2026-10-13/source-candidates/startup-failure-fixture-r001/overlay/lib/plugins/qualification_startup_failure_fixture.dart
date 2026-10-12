// Pure selection and failure boundary only: no IO, process or transport.
// This opt-in fixture is disabled in ordinary compilations.
const qualificationStartupFailureFixtureDefine = String.fromEnvironment(
  'MORROW_C28_STARTUP_FAILURE_FIXTURE', defaultValue: '',
);

final class QualificationStartupFixtureFailure implements Exception {
  const QualificationStartupFixtureFailure();
  @override
  String toString() => 'qualification-startup-failure-after-native-ready-v1';
}

const qualificationStartupFixtureFailure = QualificationStartupFixtureFailure();

enum QualificationStartupFailureFixture {
  disabled,
  afterNativeReadyV1;

  static QualificationStartupFailureFixture select({
    required String mode,
    required bool qualificationCompiled,
    required bool qualificationSelected,
  }) {
    if (mode.isEmpty) return disabled;
    if (mode != 'after-native-ready-v1' ||
        !qualificationCompiled || !qualificationSelected) {
      throw ArgumentError('Explicit compiled qualification fixture required');
    }
    return afterNativeReadyV1;
  }

  void reachNativeReady({
    required bool startupReplyValidated,
    required bool privateControlValidated,
  }) {
    if (this == disabled) return;
    if (!startupReplyValidated || !privateControlValidated) {
      throw StateError('Qualification fixture requires actual native startup');
    }
    // Only open() invokes this after its original startup exchange validation.
    throw qualificationStartupFixtureFailure;
  }
}
