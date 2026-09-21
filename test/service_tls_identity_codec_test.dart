import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/service_tls_identity_codec_native.dart';

Uint8List id(int n) => Uint8List.fromList(List.filled(32, n));
host.ResponseBuilder response(List<int> keys) {
  final response = MessageBuilder().initRoot(host.responseFactory);
  response.serviceSnapshot = id(99);
  final rows = response.initTlsIdentities(keys.length);
  for (var i = 0; i < keys.length; i++) {
    final row = rows[i].initChoice();
    row.reference = id(keys[i]);
    row.revision = 1;
    row.certificateSha256 = id(30);
  }
  return response;
}

void main() {
  test(
    'page preserves detached ordered metadata and validates snapshot/cursor',
    () {
      final out = response(List.generate(16, (i) => i + 1));
      out.serviceCursor = id(16);
      final page = ServiceTlsIdentityCodec.page(out.asReader());
      expect(page.identities, hasLength(16));
      expect(page.next, id(16));
      out.serviceSnapshot = id(80);
      expect(page.snapshot, id(99));
      expect(
        () => ServiceTlsIdentityCodec.page(
          out.asReader(),
          expectedSnapshot: id(99),
        ),
        throwsFormatException,
      );
      expect(
        () => ServiceTlsIdentityCodec.page(response([3, 2]).asReader()),
        throwsFormatException,
      );
      expect(
        () => ServiceTlsIdentityCodec.page(response([2, 2]).asReader()),
        throwsFormatException,
      );
      expect(
        () => ServiceTlsIdentityCodec.page(
          response([2]).asReader(),
          after: id(2),
        ),
        throwsFormatException,
      );
      expect(
        () => ServiceTlsIdentityCodec.page(
          response(List.generate(17, (i) => i + 1)).asReader(),
        ),
        throwsFormatException,
      );
      final short = response([1])..serviceCursor = id(1);
      expect(
        () => ServiceTlsIdentityCodec.page(short.asReader()),
        throwsFormatException,
      );
      expect(
        ServiceTlsIdentityCodec.page(response([]).asReader()).identities,
        isEmpty,
      );
    },
  );
  test(
    'receipt must match expected reference revision digest and disabled state',
    () {
      final out = response([1]);
      Object decode({
        int ref = 1,
        int rev = 1,
        int digest = 30,
        bool disabled = false,
      }) => ServiceTlsIdentityCodec.saved(
        out.asReader(),
        reference: id(ref),
        revision: BigInt.from(rev),
        certificateSha256: id(digest),
        disabled: disabled,
      );
      expect(() => decode(), returnsNormally);
      expect(() => decode(ref: 2), throwsFormatException);
      expect(() => decode(rev: 2), throwsFormatException);
      expect(() => decode(digest: 2), throwsFormatException);
      expect(() => decode(disabled: true), throwsFormatException);
      out.initTlsIdentities(0);
      expect(() => decode(), throwsFormatException);
    },
  );
  test('unexpected material and malformed identity are rejected', () {
    for (final mutate in <void Function(host.ResponseBuilder)>[
      (r) => r.issuedToken = id(4),
      (r) => r.payload = id(4),
      (r) => r.initServiceTls(),
      (r) => r.serviceSnapshot = Uint8List(0),
      (r) => r.initTlsIdentities(1),
      (r) {
        final choice = r.initTlsIdentities(1)[0].initChoice();
        choice.reference = id(1);
        choice.certificateSha256 = id(2);
        choice.revision = -1;
      },
    ]) {
      final out = response([1]);
      mutate(out);
      expect(
        () => ServiceTlsIdentityCodec.page(out.asReader()),
        throwsFormatException,
      );
    }
  });
}
