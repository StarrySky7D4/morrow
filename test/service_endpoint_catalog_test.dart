import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/service_endpoint_catalog.dart';
import 'http_task_manager_test.dart' show FakeEndpoints, endpoint, identity;

void main() {
  test(
    'complete pages own references and policies before later mutation',
    () async {
      var index = 0;
      final first = endpoint(reference: 1), second = endpoint(reference: 2);
      final fake = FakeEndpoints()
        ..page = () async {
          return index++ == 0
              ? EndpointPage(
                  entries: [first],
                  snapshot: identity(7),
                  next: identity(1),
                )
              : EndpointPage(entries: [second], snapshot: identity(7));
        };
      final result = await loadServiceEndpoints(fake);
      expect(index, 2);
      expect(result.map((e) => e.reference), [identity(1), identity(2)]);
      first.reference.fillRange(0, 32, 0);
      first.policy.packageDigest.fillRange(0, 32, 0);
      expect(result.first.reference, identity(1));
      expect(result.first.policy.packageDigest, identity(1));
      expect(() => result.clear(), throwsUnsupportedError);
    },
  );
  test('changed snapshot rejects without publishing partial results', () async {
    var index = 0;
    final fake = FakeEndpoints()
      ..page = () async => index++ == 0
          ? EndpointPage(
              entries: [endpoint(reference: 1)],
              snapshot: identity(7),
              next: identity(1),
            )
          : EndpointPage(
              entries: [endpoint(reference: 2)],
              snapshot: identity(8),
            );
    await expectLater(loadServiceEndpoints(fake), throwsFormatException);
    expect(index, 2);
  });
  test('duplicate and out-of-order rows reject', () async {
    for (final rows in [
      [endpoint(reference: 2), endpoint(reference: 2)],
      [endpoint(reference: 3), endpoint(reference: 2)],
    ]) {
      final fake = FakeEndpoints()..entries = rows;
      await expectLater(loadServiceEndpoints(fake), throwsFormatException);
    }
  });
  test('repeated cursor is bounded', () async {
    var reads = 0;
    final fake = FakeEndpoints()
      ..page = () async {
        reads++;
        return EndpointPage(
          entries: [],
          snapshot: identity(7),
          next: identity(1),
        );
      };
    await expectLater(loadServiceEndpoints(fake), throwsFormatException);
    expect(reads, 2);
  });
  test('empty advancing pages stop at the total page bound', () async {
    var reads = 0;
    final fake = FakeEndpoints()
      ..page = () async {
        final next = Uint8List(32);
        ByteData.sublistView(next).setUint32(0, ++reads, Endian.big);
        return EndpointPage(entries: [], snapshot: identity(7), next: next);
      };
    await expectLater(loadServiceEndpoints(fake), throwsFormatException);
    expect(reads, 512);
  });
  test('oversized pages fail before exposing choices', () async {
    final fake = FakeEndpoints()
      ..entries = [
        endpoint(reference: 1),
        endpoint(reference: 2),
        endpoint(reference: 3),
      ];
    await expectLater(loadServiceEndpoints(fake), throwsFormatException);
  });
}
