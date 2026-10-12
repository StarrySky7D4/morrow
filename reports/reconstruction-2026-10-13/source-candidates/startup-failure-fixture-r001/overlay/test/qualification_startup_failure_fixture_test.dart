import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/qualification_startup_failure_fixture.dart';

void main() {
  test('empty fixture mode preserves ordinary startup without qualification', () {
    final fixture = QualificationStartupFailureFixture.select(mode: '',
      qualificationCompiled: false, qualificationSelected: false);
    expect(fixture, QualificationStartupFailureFixture.disabled);
    expect(() => fixture.reachNativeReady(startupReplyValidated: false,
      privateControlValidated: false), returnsNormally);
  });
  test('fixture rejects unknown mode and either missing qualification gate', () {
    for (final row in <(String, bool, bool)>[
      ('unknown', true, true), ('after-native-ready-v1', false, true),
      ('after-native-ready-v1', true, false),
    ]) {
      expect(() => QualificationStartupFailureFixture.select(mode: row.$1,
        qualificationCompiled: row.$2, qualificationSelected: row.$3),
        throwsArgumentError);
    }
  });
  test('selected fixture cannot report sentinel before both native barriers', () {
    final fixture = QualificationStartupFailureFixture.select(
      mode: 'after-native-ready-v1', qualificationCompiled: true,
      qualificationSelected: true);
    for (final row in <(bool, bool)>[(false, false), (true, false), (false, true)]) {
      expect(() => fixture.reachNativeReady(startupReplyValidated: row.$1,
        privateControlValidated: row.$2), throwsStateError);
    }
  });
  test('selected ready fixture throws exact typed primary sentinel', () {
    final fixture = QualificationStartupFailureFixture.select(
      mode: 'after-native-ready-v1', qualificationCompiled: true,
      qualificationSelected: true);
    expect(() => fixture.reachNativeReady(startupReplyValidated: true,
      privateControlValidated: true), throwsA(same(qualificationStartupFixtureFailure)));
  });
}
