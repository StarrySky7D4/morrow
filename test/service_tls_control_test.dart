import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';

ServiceTlsSelection _sel({
  String cert = 'cert.pem',
  String key = 'key.pem',
  Uint8List? digest,
}) {
  return ServiceTlsSelection(
    certificatePath: cert,
    privateKeyPath: key,
    certificateSha256: digest ?? Uint8List.fromList(List<int>.filled(32, 7)),
  );
}

void main() {
  test('constructor copies and exposes unmodifiable Uint8List', () {
    final Uint8List source = Uint8List.fromList(List<int>.filled(32, 7));
    final ServiceTlsSelection value = ServiceTlsSelection(
      certificatePath: 'cert.pem',
      privateKeyPath: 'key.pem',
      certificateSha256: source,
    );
    source[0] = 99;
    expect(value.certificateSha256[0], equals(7));
    expect(() => value.certificateSha256[0] = 1, throwsUnsupportedError);
  });

  test('invalid paths and digests throw FormatException', () {
    final invalid = <ServiceTlsSelection>[
      _sel(cert: ''),
      _sel(key: ''),
      _sel(cert: 'a${String.fromCharCode(0)}b'),
      _sel(key: 'a${String.fromCharCode(0)}b'),
      _sel(cert: List.filled(1500, '中').join()),
      _sel(key: List.filled(1500, '中').join()),
      _sel(digest: Uint8List(31)),
      _sel(digest: Uint8List(32)),
    ];
    for (final value in invalid) {
      expect(
        () => ServiceRunValidation.tlsSelection(value),
        throwsFormatException,
      );
    }
  });

  test('valid Windows and Unix paths return normally', () {
    expect(
      () => ServiceRunValidation.tlsSelection(
        _sel(cert: r'C:\cert.pem', key: r'C:\key.pem'),
      ),
      returnsNormally,
    );
    expect(
      () => ServiceRunValidation.tlsSelection(
        _sel(cert: '/cert.pem', key: '/key.pem'),
      ),
      returnsNormally,
    );
  });
}
