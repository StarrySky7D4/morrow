import 'package:flutter_test/flutter_test.dart';
import 'agent_session_controller_cases.dart';
import 'agent_session_native_cases.dart';

void main() {
  for (final scenario in [...sessionCases, ...nativeCases]) {
    test(scenario.name, scenario.run);
  }
}
